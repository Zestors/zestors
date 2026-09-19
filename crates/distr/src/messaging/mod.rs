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
//!     distr::{ClusterNode, GlobalName, RemoteAddress},
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
//! // On another node: address the actor, and call it.
//! let counter: RemoteAddress<CounterInterface> = node
//!     .cluster()
//!     .address(GlobalName::new("counter", "node-b"))?;
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
    marker::PhantomData,
    sync::{Arc, RwLock, atomic::AtomicU64},
    time::Duration,
};
use tokio_util::sync::{CancellationToken, DropGuard};
use type_sets::{AsTypeSet, Members};
use zestors_interface::Interface;
use zestors_runtime::{Context, Dyn, TypedRegistryError};

/// Whether a lookup in the registry of this process is no reason to refuse an
/// address: the actor is there and accepts what is asked, or isn't there at all.
fn check<T>(found: Result<T, TypedRegistryError>) -> Result<(), AddressError> {
    match found {
        Ok(_) | Err(TypedRegistryError::NotFound(_)) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// What a node needs to message actors on other nodes, kept inside a
/// [`Cluster`].
pub(crate) struct MessageHandler {
    call_timeout: Duration,
    /// How many lanes to a peer messages between actors are spread over.
    shards: u8,
    handlers: DashMap<Id, Arc<dyn Handler>>,
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
            running: RwLock::new(None),
            next_call: AtomicU64::new(0),
        }
    }

    fn running(&self) -> Option<Started> {
        self.running.read().expect("Not poisoned").clone()
    }
}

/// Messaging between actors: registers what this node accepts, and makes
/// [`RemoteAddress`]es to send with.
impl Cluster {
    /// Makes messages of type `M` acceptable from other nodes: they can be sent
    /// to any actor on this node that accepts them. Messages of a type that
    /// isn't registered are answered with [`RemoteError::UnknownMessage`].
    ///
    /// Only receiving needs it; sending a message doesn't.
    pub fn register<M: RemoteMessage>(&self) -> &Self {
        self.messaging()
            .handlers
            .insert(M::Id, Arc::new(Typed::<M>(PhantomData)));
        self
    }

    /// This node.
    pub fn node(&self) -> NodeName {
        self.local_member().name
    }

    /// The actor `target` on another node, to send messages to. `I` is what it
    /// accepts, see [`RemoteAddress`].
    ///
    /// Fails if an actor with that name is running in this process, and doesn't
    /// accept `I`. An actor on another node can't be looked at, so it isn't
    /// checked that it exists, or accepts `I`; that is answered when a message
    /// is sent. Use [`Cluster::address_unchecked`] to skip the check, or
    /// [`Cluster::address_dyn`] for a set of messages.
    pub fn address<I: Interface>(
        &self,
        target: GlobalName,
    ) -> Result<RemoteAddress<I>, AddressError> {
        check(target.name().typed_address::<I>())?;
        Ok(RemoteAddress::new(self.clone(), target))
    }

    /// Like [`Cluster::address`], for the actor to accept a set of messages like
    /// `Dyn<(Ping, Double)>`.
    pub fn address_dyn<S>(&self, target: GlobalName) -> Result<RemoteAddress<Dyn<S>>, AddressError>
    where
        S: AsTypeSet + Members + 'static,
    {
        check(target.name().dyn_address::<S>())?;
        Ok(RemoteAddress::new(self.clone(), target))
    }

    /// Like [`Cluster::address`], but without looking whether an actor with that
    /// name is running in this process.
    pub fn address_unchecked<C: Context>(&self, target: GlobalName) -> RemoteAddress<C> {
        RemoteAddress::new(self.clone(), target)
    }

    /// The actor `target`, whether it is on this node or another: a
    /// [`ClusterAddress::Local`] if it is on this node, else a
    /// [`ClusterAddress::Remote`]. `I` is what it accepts.
    ///
    /// For an actor on this node it has to be running, and accept `I`;
    /// otherwise this fails. For one on another node it is checked as with
    /// [`Cluster::address`].
    pub fn cluster_address<I: Interface>(
        &self,
        target: GlobalName,
    ) -> Result<ClusterAddress<I>, AddressError> {
        if *target.node() == self.node() {
            return Ok(ClusterAddress::Local(target.name().typed_address::<I>()?));
        }
        self.address(target).map(ClusterAddress::Remote)
    }

    /// Like [`Cluster::cluster_address`], for the actor to accept a set of
    /// messages like `Dyn<(Ping, Double)>`.
    pub fn cluster_address_dyn<S>(
        &self,
        target: GlobalName,
    ) -> Result<ClusterAddress<Dyn<S>>, AddressError>
    where
        S: AsTypeSet + Members + 'static,
    {
        if *target.node() == self.node() {
            return Ok(ClusterAddress::Local(target.name().dyn_address::<S>()?));
        }
        self.address_dyn(target).map(ClusterAddress::Remote)
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
