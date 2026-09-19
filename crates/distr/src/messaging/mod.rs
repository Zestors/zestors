//! Messages to actors on other nodes.
//!
//! A [`RemoteMessage`] is sent to a [`GlobalPid`] through a [`RemoteAddress`];
//! the node that hosts the actor decodes it and delivers it like any local
//! message, and sends the reply back.
//!
//! Which message types a node accepts is decided by registering them with
//! [`Remote::register`]. Any registered message can then reach any local actor
//! that accepts it, addressed by its [`Pid`](zestors_runtime::Pid).
//!
//! Nothing here requires a message to be serde: it must be [`Encode`] and
//! [`Decode`], which every serde type is, and can be by hand for anything else.
//! [`Message`](zestors_interface::Message) itself is unchanged.
//!
//! ```no_run
//! use serde::{Deserialize, Serialize};
//! use zestors::{
//!     distr::{ClusterNode, GlobalPid, RemoteAddress},
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
//! node.remote().register::<Double>();
//!
//! // On another node: address the actor, and call it.
//! let counter: RemoteAddress<CounterInterface> = node
//!     .remote()
//!     .address(GlobalPid::new("counter", "node-b"));
//! let doubled = counter.call(Double(21)).await?;
//! # Ok(())
//! # }
//! ```

mod codec;
mod error;
mod message;
mod receive;
mod send;
mod wire;

pub use codec::{Decode, DecodeError, Encode, EncodeError};
pub use error::{RemoteCallError, RemoteCastError, RemoteError, RemoteReplyError};
pub use message::RemoteMessage;
pub use send::{RemoteAccepts, RemoteAddress, RemoteCallOptions, RemoteReceipt, RemoteReply};

use crate::{
    Cluster,
    link::{Links, Protocol},
};
use crate::{GlobalPid, Id};
use dashmap::DashMap;
use receive::{Handler, Typed};
use send::Pending;
use std::{
    marker::PhantomData,
    sync::{Arc, RwLock, atomic::AtomicU64},
    time::Duration,
};
use tokio_util::sync::{CancellationToken, DropGuard};
use zestors_runtime::Context;

/// How many pipes to a peer messages between actors are spread over.
const SHARDS: u8 = 4;

/// A node's messaging between actors: registers what it accepts, and makes
/// [`RemoteAddress`]es to send with. Get it from
/// [`ClusterNode::remote`](crate::ClusterNode::remote).
///
/// Cheap to clone, and usable before the node has started; sending fails until
/// it has.
#[derive(Clone)]
pub struct Remote {
    shared: Arc<Shared>,
}

struct Shared {
    cluster: Cluster,
    call_timeout: Duration,
    handlers: DashMap<Id, Arc<dyn Handler>>,
    /// Set while the node runs.
    running: RwLock<Option<Running>>,
    next_call: AtomicU64,
}

/// What is there once the node runs.
#[derive(Clone)]
struct Running {
    links: Links,
    pending: Arc<Pending>,
}

impl Shared {
    fn running(&self) -> Option<Running> {
        self.running.read().expect("Not poisoned").clone()
    }
}

impl Remote {
    pub(super) fn new(cluster: Cluster, call_timeout: Duration) -> Self {
        Self {
            shared: Arc::new(Shared {
                cluster,
                call_timeout,
                handlers: DashMap::new(),
                running: RwLock::new(None),
                next_call: AtomicU64::new(0),
            }),
        }
    }

    /// Makes messages of type `M` acceptable from other nodes: they can be sent
    /// to any actor on this node that accepts them. Messages of a type that
    /// isn't registered are answered with [`RemoteError::UnknownMessage`].
    ///
    /// Only receiving needs it; sending a message doesn't.
    pub fn register<M: RemoteMessage>(&self) -> &Self {
        self.shared
            .handlers
            .insert(M::Id, Arc::new(Typed::<M>(PhantomData)));
        self
    }

    /// The actor `target`, to send messages to. `C` is what it accepts, see
    /// [`RemoteAddress`].
    pub fn address<C: Context>(&self, target: GlobalPid) -> RemoteAddress<C> {
        RemoteAddress::new(self.clone(), target)
    }

    /// Starts taking in messages, and sends what is asked to, until the returned
    /// [`Service`] is stopped.
    pub(super) fn start(&self, links: Links) -> Service {
        let running = Running {
            links: links.clone(),
            pending: Arc::new(Pending::default()),
        };
        let token = CancellationToken::new();
        let serving = receive::serve(
            self.shared.clone(),
            running.clone(),
            links.subscribe(Protocol::ACTORS),
            links.peer_events(),
            self.shared.cluster.subscribe(),
        );
        tokio::spawn(token.clone().run_until_cancelled_owned(async move {
            serving.await;
        }));
        *self.shared.running.write().expect("Not poisoned") = Some(running);
        Service {
            shared: self.shared.clone(),
            _stop: token.drop_guard(),
        }
    }
}

/// [`Remote`] while the node runs. Stops when stopped or dropped.
pub(super) struct Service {
    shared: Arc<Shared>,
    _stop: DropGuard,
}

impl Service {
    /// Stops taking in messages, and gives up on the calls still waiting.
    pub(super) fn stop(self) {
        if let Some(running) = self.shared.running.write().expect("Not poisoned").take() {
            running.pending.fail_all();
        }
    }
}
