//! [`RemoteActorOps`]: what [`ActorOps`](zestors_runtime::ActorOps) offers for
//! an actor on another node.

use super::{
    RemoteAddress, RemoteMessage, RemoteOpError,
    ops::{InfoOp, PingOp, RemoteInfo, SignalOp},
};
use crate::{GlobalName, Id, NodeId};
use jiff::Zoned;
use std::future::Future;
use zestors_runtime::{ActorStatus, ChannelSnapshot, Context, Signal};

/// A reference to an actor on another node, which [`RemoteActorOps`] works on.
///
/// Implement this trait, and [`RemoteActorOps`] is automatically implemented for
/// your type.
pub trait RemoteActorRef: Sync {
    /// The [`Context`] of the associated actor.
    type Ctx: Context;

    /// The [`RemoteAddress`] of the associated actor.
    fn remote_ref(&self) -> &RemoteAddress<Self::Ctx>;
}

impl<C: Context> RemoteActorRef for RemoteAddress<C> {
    type Ctx = C;

    fn remote_ref(&self) -> &RemoteAddress<C> {
        self
    }
}

/// Operations on actors on other nodes: the counterpart of
/// [`ActorOps`](zestors_runtime::ActorOps) for a [`RemoteAddress`]. This trait
/// is sealed, and is implemented automatically for any type that implements
/// [`RemoteActorRef`].
///
/// Everything here asks the node the actor is on, so it is async and can fail.
/// Each method that reads the actor's state asks again; to read several things
/// consistently, get a [`RemoteInfo`] with [`RemoteActorOps::info`] and read
/// them from that.
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
        async move { self.remote_ref().call_op(SignalOp(signal)).await }
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
        async move { self.remote_ref().call_op(PingOp).await }
    }

    /// Asks about the actor's state, everything at one instant.
    fn info(&self) -> impl Future<Output = Result<RemoteInfo, RemoteOpError>> + Send {
        async move { self.remote_ref().call_op(InfoOp).await }
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
    /// registered, see [`RemoteInfo::accepts`].
    fn members(&self) -> impl Future<Output = Result<Vec<Id>, RemoteOpError>> + Send {
        async move { Ok(self.info().await?.accepts) }
    }

    /// Whether the actor accepts messages of type `M`. `false` also if the
    /// actor's node hasn't registered it.
    fn accepts<M: RemoteMessage>(
        &self,
    ) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        self.accepts_id(M::Id)
    }

    /// Whether the actor accepts messages with the id `id`, see
    /// [`RemoteActorOps::accepts`].
    fn accepts_id(&self, id: Id) -> impl Future<Output = Result<bool, RemoteOpError>> + Send {
        async move { Ok(self.info().await?.accepts.contains(&id)) }
    }

    /// Whether the actor accepts every message type in `ids`.
    fn is_superset_of<'a>(
        &'a self,
        ids: &'a [Id],
    ) -> impl Future<Output = Result<bool, RemoteOpError>> + Send + 'a {
        async move {
            let accepts = self.info().await?.accepts;
            Ok(ids.iter().all(|id| accepts.contains(id)))
        }
    }

    /// The actor's name and node.
    fn target(&self) -> &GlobalName {
        self.remote_ref().target()
    }

    /// The node the actor is on.
    fn node(&self) -> &NodeId {
        self.remote_ref().target().node()
    }
}

impl<T: RemoteActorRef> RemoteActorOps for T {}

mod sealed {
    pub trait Sealed {}
    impl<T: super::RemoteActorRef> Sealed for T {}
}
