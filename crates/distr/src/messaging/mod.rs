//! Messages to actors on other nodes.
//!
//! A [`RemoteMessage`] is sent to a [`GlobalName`] through a [`RemoteAddress`];
//! the node that hosts the actor decodes it and delivers it like any local
//! message, and sends the reply back. A [`ClusterAddress`] is the same for an
//! actor that may be on this node too, which is then reached without leaving
//! the process.
//!
//! Which message types a node accepts is decided by registering them with
//! [`Cluster::register`]. Any registered message can then reach any local actor
//! that accepts it, addressed by its [`Name`](zestors_runtime::Name).
//!
//! Nothing here requires a message to be serde: it must be [`Encode`] and
//! [`Decode`], which every serde type is, and can be by hand for anything else.
//! [`Message`](zestors_interface::Message) itself is unchanged.
//!
//! A reply channel can be part of a message too: see [`RemoteRequest`].
//!
//! ```no_run
//! use serde::{Deserialize, Serialize};
//! use zestors::{
//!     distr::{ClusterAddress, ClusterNode, GlobalName},
//!     interface::{Envelope, Interface, Message},
//!     prelude::*,
//! };
//!
//! // A message that can cross the network: it has a stable id, and serde.
//! #[derive(Message, StableId, Serialize, Deserialize, Debug)]
//! #[msg(reply = u32, id = "1c3d5e7f-2a4b-4c6d-8e0f-a1b2c3d4e5f6")]
//! struct Double(u32);
//!
//! #[derive(Interface, Debug)]
//! enum CounterInterface {
//!     Double(Envelope<Double>),
//! }
//!
//! # async fn example(node: ClusterNode) -> Result<(), Box<dyn std::error::Error>> {
//! // On the node that runs the actor: accept the message from other nodes.
//! node.cluster().register::<Double>();
//!
//! // On another node: address the actor, and call it. This looks for the
//! // actor there, checking that it accepts the messages registered here.
//! let counter: ClusterAddress<CounterInterface> = node
//!     .cluster()
//!     .address(GlobalName::new("counter", "node-b"))
//!     .await?;
//! let doubled = counter.call(Double(21)).await?;
//! # Ok(())
//! # }
//! ```

mod accepts;
mod actor_ops;
mod cluster_address;
mod codec;
mod context;
mod error;
mod handler;
mod message;
mod ops;
mod receive;
mod reply;
mod request;
mod send;
mod wire;

pub use accepts::{RemoteAccepts, RemoteCallOptions};
pub use actor_ops::{RemoteActorOps, RemoteActorRef, Route};
pub use cluster_address::ClusterAddress;
pub use codec::{Decode, DecodeError, Encode, EncodeError};
pub use error::{
    AddressError, RemoteCallError, RemoteCastError, RemoteError, RemoteOpError, RemoteReplyError,
};
pub use message::RemoteMessage;
pub use ops::RemoteInfo;
pub use reply::{RemoteReceipt, RemoteReply};
pub use request::RemoteRequest;
pub use send::RemoteAddress;

use crate::{
    Cluster, GlobalName, Id, NodeName,
    link::{Links, Protocol},
};
use dashmap::DashMap;
use handler::{Handler, Typed};
use reply::Pending;
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
pub(crate) struct MessageHandler {
    call_timeout: Duration,
    /// How many lanes to a peer messages between actors are spread over.
    shards: u8,
    handlers: DashMap<Id, Arc<dyn Handler>>,
    /// The id each registered message type goes by on the wire.
    type_ids: DashMap<TypeId, Id>,
    /// Set while the node runs.
    running: RwLock<Option<Started>>,
    next_call: AtomicU64,
}

/// What is there once the node runs.
#[derive(Clone)]
struct Started {
    links: Links,
    pending: Arc<Pending>,
}

impl MessageHandler {
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

    fn running(&self) -> Option<Started> {
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
        let ids: Vec<Id> = types
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
    pub(super) fn start(&self, links: Links) -> Serving {
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
pub(super) struct Serving {
    cluster: Cluster,
    _stop: DropGuard,
}

impl Serving {
    /// Stops taking in messages, and gives up on the calls still waiting.
    pub(super) fn stop(self) {
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
