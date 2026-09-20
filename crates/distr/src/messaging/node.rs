//! This node's side of messaging: what it accepts from other nodes, the
//! addresses it makes, and the state it has while it runs.

use super::{
    AddressError, ClusterActorOps as _, ClusterAddress, LocalAddress, RemoteAddress, RemoteError,
    RemoteOpError, RemoteReplyError, RemoteSet, dispatch::Handlers, pending::Pending, receive,
};
use crate::{
    Cluster, GlobalName, MessageId,
    link::{Links, Protocol},
};
use std::{
    sync::{Arc, RwLock, atomic::AtomicU64},
    time::Duration,
};
use tokio_util::sync::{CancellationToken, DropGuard};
use type_sets::{AsTypeSet, Members};
use zestors_interface::Interface;
use zestors_runtime::{Address, Context, Dyn, Registry};

/// What a node needs to message actors on other nodes, kept inside a
/// [`Cluster`].
pub(crate) struct CommunicationView {
    pub(super) call_timeout: Duration,
    /// How many lanes to a peer messages between actors are spread over.
    pub(super) shards: u8,
    /// The messages this node accepts, fixed when it was built.
    pub(super) handlers: Handlers,
    /// Set while the node runs.
    pub(super) running: RwLock<Option<Started>>,
    /// Numbers the answers this node is waiting for. Both a call and a request
    /// handed to another node inside a message draw from it, because both are
    /// answered by a [`Frame::Reply`](super::frame::Frame::Reply) and looked up
    /// in the one [`Pending`] table: two counters would collide there.
    pub(super) next_call: AtomicU64,
}

/// What is there once the node runs.
#[derive(Clone)]
pub(super) struct Started {
    pub(super) links: Links,
    pub(super) pending: Arc<Pending>,
}

impl CommunicationView {
    pub(crate) fn new(call_timeout: Duration, shards: u8, handlers: Handlers) -> Self {
        Self {
            call_timeout,
            shards,
            handlers,
            running: RwLock::new(None),
            next_call: AtomicU64::new(0),
        }
    }

    pub(super) fn running(&self) -> Option<Started> {
        self.running.read().expect("Not poisoned").clone()
    }
}

/// Messaging between actors: registers what this node accepts, and makes
/// [`RemoteAddress`]es to send with.
impl Cluster {
    /// The ids `address` accepts, among the messages this node was built with,
    /// sorted. The one answer to that question, so that a node says the same
    /// about an actor whether it is asked from here or over the network.
    pub(crate) fn accepted_ids(&self, address: &Address) -> Vec<MessageId> {
        self.messaging().handlers.accepted_ids(address)
    }

    /// Whether `address` accepts the message `id`, without building the list.
    pub(crate) fn accepts_id(&self, address: &Address, id: MessageId) -> bool {
        self.messaging().handlers.accepts_id(address, id)
    }

    /// An actor on this node as a [`ClusterAddress`], so that it can be
    /// operated on the same way as one anywhere else in the cluster. The
    /// cluster is needed because the operations that go by
    /// [`MessageId`] answer from its registry.
    pub fn local_address<C: Context>(&self, address: Address<C>) -> ClusterAddress<C> {
        ClusterAddress::Local(LocalAddress::new(self.clone(), address))
    }

    /// The actor `target`, whether it is on this node or another: a
    /// [`ClusterAddress::Local`] if it is on this node, else a
    /// [`ClusterAddress::Remote`]. `I` is what it accepts.
    ///
    /// The actor is looked up where it is: in the registry of this node, or by
    /// asking the node it is on. It has to be running, and accept `I`.
    ///
    /// For an actor on another node, that node has to be a reachable member,
    /// and every message in `I` is asked about — which is why `I`'s messages
    /// must all be able to cross the network, see [`RemoteSet`].
    ///
    /// Use [`Cluster::address_dyn`] for a set of messages instead of a whole
    /// interface.
    pub async fn address<I: Interface>(
        &self,
        target: GlobalName,
    ) -> Result<ClusterAddress<I>, AddressError>
    where
        I::Set: RemoteSet,
    {
        if target.node() == self.name() {
            let address = Registry::local().get_typed::<I>(target.name())?;
            return Ok(self.local_address(address));
        }
        self.resolve(target, <I::Set as RemoteSet>::REMOTE_IDS)
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
        S: AsTypeSet + Members + RemoteSet + 'static,
    {
        if target.node() == self.name() {
            let address = Registry::local().get_dyn::<S>(target.name())?;
            return Ok(self.local_address(address));
        }
        self.resolve(target, S::REMOTE_IDS)
            .await
            .map(ClusterAddress::Remote)
    }

    /// Asks the node `target` is on whether the actor is there, and accepts
    /// the messages with these ids.
    async fn resolve<C: Context>(
        &self,
        target: GlobalName,
        ids: &'static [MessageId],
    ) -> Result<RemoteAddress<C>, AddressError> {
        let name = target.name().clone();
        let address = RemoteAddress::new(self.clone(), target);
        match address.is_superset_of(ids).await {
            Ok(true) => Ok(address),
            Ok(false) => Err(AddressError::TypeMismatch(name)),
            Err(RemoteOpError::Reply(RemoteReplyError::Remote(RemoteError::NoSuchActor))) => {
                Err(AddressError::NoSuchActor(name))
            }
            Err(error) => Err(AddressError::Remote(error)),
        }
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

/// Messaging while the node runs. Stops when dropped, however that happens:
/// the node exiting on its own, or its task being cancelled part way.
pub(crate) struct Serving {
    cluster: Cluster,
    _stop: DropGuard,
}

impl Drop for Serving {
    /// Stops taking in messages, and gives up on the calls still waiting.
    ///
    /// This is in `Drop` and not a method that the node remembers to call, so
    /// that a node which stops without getting that far still tells whoever is
    /// waiting, rather than leaving them to wait out the call timeout.
    fn drop(&mut self) {
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
