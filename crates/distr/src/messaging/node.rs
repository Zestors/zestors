//! This node's side of messaging: what it accepts from other nodes, the
//! addresses it makes, and the state it has while it runs.

use super::{
    AddressError, ClusterAddress, RemoteActorOps as _, RemoteAddress, RemoteError, RemoteMessage,
    RemoteOpError, RemoteReplyError,
    dispatch::{Handler, Typed},
    ops,
    pending::Pending,
    receive,
};
use crate::{
    Cluster, GlobalName, MessageId, NodeName,
    link::{Links, Protocol},
};
use dashmap::DashMap;
use std::{
    any::TypeId,
    marker::PhantomData,
    sync::{Arc, RwLock, atomic::AtomicU64},
    time::Duration,
};
use tokio_util::sync::{CancellationToken, DropGuard};
use type_sets::{AsTypeSet, Members};
use zestors_interface::Interface;
use zestors_runtime::{Context, Dyn};

/// What a node needs to message actors on other nodes, kept inside a
/// [`Cluster`].
pub(crate) struct CommunicationView {
    pub(super) call_timeout: Duration,
    /// How many lanes to a peer messages between actors are spread over.
    pub(super) shards: u8,
    pub(super) handlers: DashMap<MessageId, Arc<dyn Handler>>,
    /// The id each registered message type goes by on the wire.
    pub(super) type_ids: DashMap<TypeId, MessageId>,
    /// Set while the node runs.
    pub(super) running: RwLock<Option<Started>>,
    pub(super) next_call: AtomicU64,
}

/// What is there once the node runs.
#[derive(Clone)]
pub(super) struct Started {
    pub(super) links: Links,
    pub(super) pending: Arc<Pending>,
}

impl CommunicationView {
    pub(crate) fn new(call_timeout: Duration, shards: u8) -> Self {
        let handlers = DashMap::new();
        ops::register(&handlers);
        Self {
            call_timeout,
            shards,
            handlers,
            type_ids: DashMap::new(),
            running: RwLock::new(None),
            next_call: AtomicU64::new(0),
        }
    }

    pub(super) fn running(&self) -> Option<Started> {
        self.running.read().expect("Not poisoned").clone()
    }

    pub(super) fn register<M: RemoteMessage>(&self) {
        self.handlers
            .insert(M::Id, Arc::new(Typed::<M>(PhantomData)));
        self.type_ids.insert(TypeId::of::<M>(), M::Id);
    }
}

/// Messaging between actors: registers what this node accepts, and makes
/// [`RemoteAddress`]es to send with.
impl Cluster {
    /// Makes messages of type `M` acceptable from other nodes: they can be sent
    /// to any actor on this node that accepts them. Messages of a type that
    /// isn't registered are answered with [`RemoteError::UnknownMessage`].
    ///
    /// Also what makes [`Cluster::address`] check that an actor on another node
    /// accepts `M`: it can only tell the other node about the messages that are
    /// registered here. Sending a message needn't be registered otherwise.
    pub fn register<M: RemoteMessage>(&self) -> &Self {
        self.messaging().register::<M>();
        self
    }

    /// This node.
    pub fn name(&self) -> NodeName {
        self.local_member().name
    }

    /// The actor `target`, whether it is on this node or another: a
    /// [`ClusterAddress::Local`] if it is on this node, else a
    /// [`ClusterAddress::Remote`]. `I` is what it accepts.
    ///
    /// The actor is looked up where it is: in the registry of this node, or by
    /// asking the node it is on. It has to be running, and accept `I`. For an
    /// actor on another node, that needs the node to be a reachable member,
    /// and it is only checked to accept the messages in `I` that are
    /// [registered](Cluster::register) on this node, and on its own node. Use [`Cluster::address_unchecked`] to skip the check, or
    /// [`Cluster::address_dyn`] for a set of messages.
    pub async fn address<I: Interface>(
        &self,
        target: GlobalName,
    ) -> Result<ClusterAddress<I>, AddressError> {
        if *target.node() == self.name() {
            return Ok(ClusterAddress::Local(target.name().typed_address::<I>()?));
        }
        self.resolve(target, <I::Set as Members>::members())
            .await
            .map(ClusterAddress::Remote)
    }

    /// Like [`Cluster::address`], for the actor to accept a set of messages like
    /// `Dyn<(Ping, Double)>`.
    pub async fn address_dyn<S>(
        &self,
        target: GlobalName,
    ) -> Result<ClusterAddress<Dyn<S>>, AddressError>
    where
        S: AsTypeSet + Members + 'static,
    {
        if *target.node() == self.name() {
            return Ok(ClusterAddress::Local(target.name().dyn_address::<S>()?));
        }
        self.resolve(target, S::members())
            .await
            .map(ClusterAddress::Remote)
    }

    /// Asks the node `target` is on whether the actor is there, and accepts
    /// the messages of `types`.
    async fn resolve<C: Context>(
        &self,
        target: GlobalName,
        types: &[TypeId],
    ) -> Result<RemoteAddress<C>, AddressError> {
        // A message that isn't registered has no id to tell the other node, and
        // can't be sent through the address either.
        let ids: Vec<MessageId> = types
            .iter()
            .filter_map(|ty| self.messaging().type_ids.get(ty).map(|id| *id))
            .collect();
        let name = target.name().clone();
        let address = RemoteAddress::new(self.clone(), target);
        match address.is_superset_of(&ids).await {
            Ok(true) => Ok(address),
            Ok(false) => Err(AddressError::TypeMismatch(name)),
            Err(RemoteOpError::Reply(RemoteReplyError::Remote(RemoteError::NoSuchActor))) => {
                Err(AddressError::NoSuchActor(name))
            }
            Err(error) => Err(AddressError::Remote(error)),
        }
    }

    /// Like [`Cluster::address`], but without looking for the actor, so that it
    /// isn't async and doesn't need the node to be reachable. What is wrong is
    /// then answered when a message is sent.
    pub fn address_unchecked<C: Context>(&self, target: GlobalName) -> RemoteAddress<C> {
        RemoteAddress::new(self.clone(), target)
    }

    /// Starts taking in messages, and sends what is asked to, until the returned
    /// [`Serving`] is stopped.
    pub(crate) fn start(&self, links: Links) -> Serving {
        let running = Started {
            links: links.clone(),
            pending: Arc::new(Pending::default()),
        };
        let token = CancellationToken::new();
        let serving = receive::serve(
            self.clone(),
            running.clone(),
            links.subscribe(Protocol::ACTORS),
            links.peer_events(),
            self.subscribe(),
        );
        tokio::spawn(token.clone().run_until_cancelled_owned(async move {
            serving.await;
        }));
        *self.messaging().running.write().expect("Not poisoned") = Some(running);
        Serving {
            cluster: self.clone(),
            _stop: token.drop_guard(),
        }
    }
}

/// Messaging while the node runs. Stops when stopped or dropped.
pub(crate) struct Serving {
    cluster: Cluster,
    _stop: DropGuard,
}

impl Serving {
    /// Stops taking in messages, and gives up on the calls still waiting.
    pub(crate) fn stop(self) {
        if let Some(running) = self
            .cluster
            .messaging()
            .running
            .write()
            .expect("Not poisoned")
            .take()
        {
            running.pending.fail_all();
        }
    }
}
