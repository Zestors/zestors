//! Operations on an actor, as opposed to messages for it: signalling it,
//! checking that it is alive, and asking about its state. They are what
//! [`RemoteActorOps`](super::RemoteActorOps) sends.
//!
//! They are messages like any other on the wire, with ids of their own, and
//! every node handles them without them being registered. Unlike messages,
//! they are for the actor's channel and not its mailbox: they don't wait
//! behind the messages queued for the actor, just as signals don't locally.
//! That is why they opt out of `auto_register`, which would make them messages
//! for the mailbox.

use super::{
    RemoteActorRef, RemoteError, RemoteMessage, RemoteOpError, RemoteReplyError, Route,
    dispatch::{Builtin, Handler, Operation},
};
use crate::{Cluster, MessageId, StableId};
use dashmap::DashMap;
use jiff::Zoned;
use serde::{Deserialize, Serialize};
use std::{future::Future, marker::PhantomData, sync::Arc};
use zestors_interface::Message;
use zestors_runtime::{ActorStatus, Address, ChannelSnapshot, Name, Signal, prelude::*};

/// Sends a [`Signal`] to the actor. Answered with whether it was accepted:
/// `false` if the actor was already exiting or dead.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = bool, id = "b5c3f7a0-5f0e-4b0f-9f7e-2f6c1f0a0001", no_auto_register)]
#[zestors(interface_path = "zestors_interface", distr_path = "crate")]
pub(super) struct SignalOp(pub(super) Signal);

/// Waits for the actor to process a signal. Answered once it has.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = (), id = "b5c3f7a0-5f0e-4b0f-9f7e-2f6c1f0a0002", no_auto_register)]
#[zestors(interface_path = "zestors_interface", distr_path = "crate")]
pub(super) struct PingOp;

/// Asks about the state of the actor.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = RemoteInfo, id = "b5c3f7a0-5f0e-4b0f-9f7e-2f6c1f0a0003", no_auto_register)]
#[zestors(interface_path = "zestors_interface", distr_path = "crate")]
pub(super) struct InfoOp;

/// The state of an actor on another node, at one instant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteInfo {
    /// The actor's status, queue lengths and spawn and exit history. The
    /// timestamps are the clock of the node the actor is on.
    pub snapshot: ChannelSnapshot,
    /// Whether the actor's mailbox is full, so that sending to it waits.
    pub reached_backpressure: bool,
    /// The ids of the message types that the actor accepts and that its node
    /// has registered with [`Cluster::register`](crate::Cluster::register). `None`
    /// for an actor on this node, which can't tell.
    pub accepts: Option<Vec<MessageId>>,
}

impl Operation for SignalOp {
    fn run(
        self,
        address: Address,
        _: Cluster,
    ) -> super::dispatch::BoxFuture<Result<bool, RemoteError>> {
        Box::pin(async move { Ok(address.signal(self.0)) })
    }
}

impl Operation for PingOp {
    fn run(
        self,
        address: Address,
        _: Cluster,
    ) -> super::dispatch::BoxFuture<Result<(), RemoteError>> {
        Box::pin(async move { address.ping().await.map_err(|_| RemoteError::NoReply) })
    }
}

impl Operation for InfoOp {
    fn run(
        self,
        address: Address,
        cluster: Cluster,
    ) -> super::dispatch::BoxFuture<Result<RemoteInfo, RemoteError>> {
        Box::pin(async move {
            let mut accepts: Vec<MessageId> = cluster
                .messaging()
                .handlers
                .iter()
                .filter(|handler| handler.accepts(&address))
                .map(|handler| *handler.key())
                .collect();
            accepts.sort();
            Ok(RemoteInfo {
                snapshot: address.snapshot(),
                reached_backpressure: address.reached_backpressure(),
                accepts: Some(accepts),
            })
        })
    }
}

/// Makes a node handle the operations.
pub(super) fn register(handlers: &DashMap<MessageId, Arc<dyn Handler>>) {
    handlers.insert(SignalOp::Id, Arc::new(Builtin::<SignalOp>(PhantomData)));
    handlers.insert(PingOp::Id, Arc::new(Builtin::<PingOp>(PhantomData)));
    handlers.insert(InfoOp::Id, Arc::new(Builtin::<InfoOp>(PhantomData)));
}

/// Operations on actors on other nodes: the counterpart of
/// [`ActorOps`](zestors_runtime::ActorOps) for a [`RemoteAddress`] or a
/// [`ClusterAddress`](super::ClusterAddress). This trait is sealed, and is
/// implemented automatically for any type that implements [`RemoteActorRef`].
///
/// Everything here asks the node the actor is on, so it is async and can fail.
/// If the actor is on this node, which a [`ClusterAddress`](super::ClusterAddress)
/// can be for, the answer is read from it directly and the future is ready at
/// once. Each method that reads the actor's state asks again; to read several
/// things consistently, get a [`RemoteInfo`] with [`RemoteActorOps::info`] and
/// read them from that.
///
/// Sending messages is done with
/// [`RemoteAccepts`](super::RemoteAccepts), and waiting for an actor's status to
/// change isn't supported yet.
///
/// The operations aren't queued behind the messages waiting for the actor.
pub trait RemoteActorOps: RemoteActorRef + sealed::Sealed {
    /// Sends the given [`Signal`] to the actor. Returns `false` if the actor
    /// was already exiting or dead.
    fn signal(&self, signal: Signal) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        async move {
            match self.route() {
                Route::Local(address) => Ok(address.signal(signal)),
                Route::Remote(address) => address.call_op(SignalOp(signal)).await,
            }
        }
    }

    /// Sends a [`Signal::Shutdown`] to the actor. Returns `false` if it was
    /// already exiting or dead.
    fn signal_shutdown(&self) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        self.signal(Signal::Shutdown)
    }

    /// Sends a [`Signal::Suspend`] to the actor. Returns `false` if it was
    /// already exiting or dead.
    fn signal_suspend(&self) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        self.signal(Signal::Suspend)
    }

    /// Sends a [`Signal::Resume`] to the actor. Returns `false` if it was
    /// already exiting or dead.
    fn signal_resume(&self) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        self.signal(Signal::Resume)
    }

    /// Waits until the actor has processed a signal. As signals are processed
    /// before the messages queued, this confirms that the actor is alive and
    /// its event loop has caught up with its signals.
    fn ping(&self) -> impl Future<Output = Result<(), RemoteOpError>> + Send {
        async move {
            match self.route() {
                Route::Local(address) => address.ping().await.map_err(|_| {
                    RemoteOpError::Reply(RemoteReplyError::Remote(RemoteError::NoReply))
                }),
                Route::Remote(address) => address.call_op(PingOp).await,
            }
        }
    }

    /// Asks about the actor's state, everything at one instant. For a local
    /// actor, [`RemoteInfo::accepts`] is `None`.
    fn info(&self) -> impl Future<Output = Result<RemoteInfo, RemoteOpError>> + Send {
        async move {
            match self.route() {
                Route::Local(address) => Ok(RemoteInfo {
                    snapshot: address.snapshot(),
                    reached_backpressure: address.reached_backpressure(),
                    accepts: None,
                }),
                Route::Remote(address) => address.call_op(InfoOp).await,
            }
        }
    }

    /// Captures a [`ChannelSnapshot`] of the actor. Its timestamps are from the
    /// clock of the actor's node.
    fn snapshot(&self) -> impl Future<Output = Result<ChannelSnapshot, RemoteOpError>> + Send {
        async move { Ok(self.info().await?.snapshot) }
    }

    /// The actor's current [`ActorStatus`].
    fn status(&self) -> impl Future<Output = Result<ActorStatus, RemoteOpError>> + Send {
        async move { Ok(self.info().await?.snapshot.status) }
    }

    /// Whether the actor's status is [`ActorStatus::Exiting`].
    fn is_exiting(&self) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        async move { Ok(self.status().await?.is_exiting()) }
    }

    /// Whether the actor's status is [`ActorStatus::Exited`].
    fn is_dead(&self) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        async move { Ok(self.status().await?.is_dead()) }
    }

    /// The number of messages currently queued for the actor.
    fn msg_len(&self) -> impl Future<Output = Result<usize, RemoteOpError>> + Send {
        async move { Ok(self.info().await?.snapshot.msg_len) }
    }

    /// Whether no messages are currently queued for the actor.
    fn msg_is_empty(&self) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        async move { Ok(self.msg_len().await? == 0) }
    }

    /// The number of signals currently queued for the actor.
    fn signal_len(&self) -> impl Future<Output = Result<usize, RemoteOpError>> + Send {
        async move { Ok(self.info().await?.snapshot.signal_len) }
    }

    /// Whether no signals are currently queued for the actor.
    fn signal_is_empty(&self) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        async move { Ok(self.signal_len().await? == 0) }
    }

    /// Whether the actor's mailbox is full, so that sending to it waits.
    fn reached_backpressure(&self) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        async move { Ok(self.info().await?.reached_backpressure) }
    }

    /// When the actor was last spawned, on the clock of its node, or `None` if
    /// it never was.
    fn last_spawned_at(&self) -> impl Future<Output = Result<Option<Zoned>, RemoteOpError>> + Send {
        async move { Ok(self.info().await?.snapshot.spawns.last().cloned()) }
    }

    /// The ids of the message types the actor accepts and that its node has
    /// registered, see [`RemoteInfo::accepts`]. Not supported for a local actor.
    fn members(&self) -> impl Future<Output = Result<Vec<MessageId>, RemoteOpError>> + Send {
        async move { self.info().await?.accepts.ok_or(RemoteOpError::Unsupported) }
    }

    /// Whether the actor accepts messages of type `M`. For a remote actor,
    /// `false` also if its node hasn't registered it.
    fn accepts<M: RemoteMessage>(
        &self,
    ) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        async move {
            match self.route() {
                Route::Local(address) => Ok(address.accepts::<M>()),
                Route::Remote(_) => self.accepts_id(M::Id).await,
            }
        }
    }

    /// Whether the actor accepts messages with the id `id`, see
    /// [`RemoteActorOps::accepts`]. Not supported for a local actor.
    fn accepts_id(
        &self,
        id: MessageId,
    ) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        async move { Ok(self.members().await?.contains(&id)) }
    }

    /// Whether the actor accepts every message type in `ids`. Not supported for
    /// a local actor.
    fn is_superset_of<'a>(
        &'a self,
        ids: &'a [MessageId],
    ) -> impl Future<Output = Result<bool, RemoteOpError>> + Send + 'a {
        async move {
            let accepts = self.members().await?;
            Ok(ids.iter().all(|id| accepts.contains(id)))
        }
    }

    /// The actor's name.
    fn name(&self) -> &Name {
        match self.route() {
            Route::Local(address) => address.name(),
            Route::Remote(address) => address.name().name(),
        }
    }
}

impl<T: RemoteActorRef> RemoteActorOps for T {}

mod sealed {
    pub trait Sealed {}
    impl<T: super::RemoteActorRef> Sealed for T {}
}
