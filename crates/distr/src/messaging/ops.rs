//! Operations on an actor, as opposed to messages for it: signalling it,
//! checking that it is alive, and asking about its state. They are what
//! [`ClusterActorOps`](super::ClusterActorOps) sends.
//!
//! They are messages like any other on the wire, with ids of their own, and
//! every node handles them without them being registered. Unlike messages,
//! they are for the actor's channel and not its mailbox: they don't wait
//! behind the messages queued for the actor, just as signals don't locally.
//! That is why they opt out of `auto_register`, which would make them messages
//! for the mailbox.

use super::{
    ClusterActorRef, ClusterOpError, ClusterReplyError, RemoteError, RemoteMessage,
    address::Target,
    dispatch::{Handlers, Operation},
};
use crate::{Cluster, MessageId, NodeName, StableId};
use jiff::Zoned;
use serde::{Deserialize, Serialize};
use std::future::Future;
use zestors_interface::Message;
use zestors_runtime::{
    ActorStatus, ActorStatusKind, Address, AsDyn, ChannelSnapshot, ExitStatus, Name, Signal,
    errors::ExitError, prelude::*,
};

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
#[msg(reply = ActorInfo, id = "b5c3f7a0-5f0e-4b0f-9f7e-2f6c1f0a0003", no_auto_register)]
#[zestors(interface_path = "zestors_interface", distr_path = "crate")]
pub(super) struct InfoOp;

/// Waits until the actor's status is one of `kinds`, and answers with it.
///
/// Unlike the others this outlives its message: it is answered whenever the
/// actor gets there, which may be never. It is sent without a deadline, and
/// called off with [`DemonitorOp`].
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = ActorStatus, id = "b5c3f7a0-5f0e-4b0f-9f7e-2f6c1f0a0006", no_auto_register)]
#[zestors(interface_path = "zestors_interface", distr_path = "crate")]
pub(super) struct MonitorOp {
    /// Names the monitor so it can be called off. Minted by the monitoring node, so
    /// it is only unique together with that node's name.
    pub(super) monitor_id: u64,
    pub(super) kinds: Vec<ActorStatusKind>,
}

/// Calls off a [`MonitorOp`], so that the node holding it can forget it. Expects
/// no reply: by the time this is sent, nobody is listening for one.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(id = "b5c3f7a0-5f0e-4b0f-9f7e-2f6c1f0a0007", no_auto_register)]
#[zestors(interface_path = "zestors_interface", distr_path = "crate")]
pub(super) struct DemonitorOp {
    pub(super) monitor_id: u64,
}

/// The state of an actor at one instant, wherever it is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorInfo {
    /// The actor's status, queue lengths and spawn and exit history. The
    /// timestamps are the clock of the node the actor is on.
    pub snapshot: ChannelSnapshot,
    /// Whether the actor's mailbox is full, so that sending to it waits.
    pub reached_backpressure: bool,
    /// The ids of the message types that the actor accepts and that its node
    /// has registered with [`ClusterConfig::register`](crate::ClusterConfig::register),
    /// sorted. A message the node hasn't registered is not listed, wherever the
    /// actor is: the node has no id to name it by.
    pub accepts: Vec<MessageId>,
}

/// Asks for the state of the actor that is live counters only: no name, no
/// spawn and exit history, no accepts list. Answers everything a caller that
/// wants one number needs, at a fraction of an [`InfoOp`]'s wire cost.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = ChannelState, id = "b5c3f7a0-5f0e-4b0f-9f7e-2f6c1f0a0004", no_auto_register)]
#[zestors(interface_path = "zestors_interface", distr_path = "crate")]
pub(super) struct StateOp;

/// The cheap half of a [`ActorInfo`]: what the actor's channel says about
/// itself right now. Not public: it is read one field at a time through
/// [`ClusterActorOps`], and a caller that wants several at one instant asks
/// for a [`ActorInfo`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ChannelState {
    pub(super) status: ActorStatus,
    pub(super) msg_len: usize,
    pub(super) signal_len: usize,
    pub(super) reached_backpressure: bool,
}

/// Asks whether the actor accepts every message in the list. The node answers
/// from its [`Handlers`](super::dispatch::Handlers) with a single bool, rather
/// than sending back the whole accepts list for the caller to scan.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = bool, id = "b5c3f7a0-5f0e-4b0f-9f7e-2f6c1f0a0005", no_auto_register)]
#[zestors(interface_path = "zestors_interface", distr_path = "crate")]
pub(super) struct AcceptsOp(pub(super) Vec<MessageId>);

impl Operation for SignalOp {
    fn run(
        self,
        address: Address,
        _: Cluster,
        _: NodeName,
    ) -> super::dispatch::BoxFuture<Result<bool, RemoteError>> {
        Box::pin(async move { Ok(address.signal(self.0)) })
    }
}

impl Operation for PingOp {
    fn run(
        self,
        address: Address,
        _: Cluster,
        _: NodeName,
    ) -> super::dispatch::BoxFuture<Result<(), RemoteError>> {
        Box::pin(async move { address.ping().await.map_err(|_| RemoteError::NoReply) })
    }
}

impl Operation for InfoOp {
    fn run(
        self,
        address: Address,
        cluster: Cluster,
        _: NodeName,
    ) -> super::dispatch::BoxFuture<Result<ActorInfo, RemoteError>> {
        Box::pin(async move {
            Ok(ActorInfo {
                snapshot: address.snapshot(),
                reached_backpressure: address.reached_backpressure(),
                accepts: cluster.accepted_ids(&address),
            })
        })
    }
}

impl Operation for StateOp {
    fn run(
        self,
        address: Address,
        _: Cluster,
        _: NodeName,
    ) -> super::dispatch::BoxFuture<Result<ChannelState, RemoteError>> {
        Box::pin(async move {
            Ok(ChannelState {
                status: address.status(),
                msg_len: address.msg_len(),
                signal_len: address.signal_len(),
                reached_backpressure: address.reached_backpressure(),
            })
        })
    }
}

impl Operation for AcceptsOp {
    fn run(
        self,
        address: Address,
        cluster: Cluster,
        _: NodeName,
    ) -> super::dispatch::BoxFuture<Result<bool, RemoteError>> {
        Box::pin(async move { Ok(cluster.accepts_ids(&address, &self.0)) })
    }
}

impl Operation for MonitorOp {
    fn run(
        self,
        address: Address,
        cluster: Cluster,
        peer: NodeName,
    ) -> super::dispatch::BoxFuture<Result<ActorStatus, RemoteError>> {
        Box::pin(async move {
            let monitors = &cluster.messaging().monitors;
            let cancelled = monitors.begin(peer.clone(), self.monitor_id);
            let reached = tokio::select! {
                status = address.monitor_any(&self.kinds) => Ok(status),
                // The monitoring_node is gone. Nothing is listening for this reply, so
                // what it says doesn't matter.
                _ = cancelled.cancelled() => Err(RemoteError::NoReply),
            };
            monitors.end(&peer, self.monitor_id);
            reached
        })
    }
}

impl Operation for DemonitorOp {
    fn run(
        self,
        _: Address,
        cluster: Cluster,
        peer: NodeName,
    ) -> super::dispatch::BoxFuture<Result<(), RemoteError>> {
        Box::pin(async move {
            // A miss is normal: the monitor may have been answered already.
            cluster.messaging().monitors.cancel(&peer, self.monitor_id);
            Ok(())
        })
    }
}

/// Makes a node handle the operations. Each claims its id like any other
/// message, so an operation given an id another one already has is caught the
/// moment a node is built — by the first test that builds one.
pub(super) fn register(handlers: &mut Handlers) {
    handlers.insert_op::<SignalOp>();
    handlers.insert_op::<PingOp>();
    handlers.insert_op::<InfoOp>();
    handlers.insert_op::<MonitorOp>();
    handlers.insert_op::<DemonitorOp>();
    handlers.insert_op::<StateOp>();
    handlers.insert_op::<AcceptsOp>();
}

/// Operations on actors on other nodes: the counterpart of
/// [`ActorOps`](zestors_runtime::ActorOps) for a
/// [`ClusterAddress`](super::ClusterAddress). This trait is sealed, and is
/// implemented automatically for any type that implements [`ClusterActorRef`].
///
/// Everything here asks the node the actor is on, so it is async and can fail.
/// If the actor is on this node, which a [`ClusterAddress`](super::ClusterAddress)
/// can be for, the answer is read from it directly and the future is ready at
/// once. Each method that reads the actor's state asks again; to read several
/// things consistently, get a [`ActorInfo`] with [`ClusterActorOps::info`] and
/// read them from that.
///
/// Sending messages is done with
/// [`ClusterAccepts`](super::ClusterAccepts), and waiting for an actor's status to
/// change isn't supported yet.
///
/// The operations aren't queued behind the messages waiting for the actor.
pub trait ClusterActorOps: ClusterActorRef + sealed::Sealed {
    /// Sends the given [`Signal`] to the actor. Returns `false` if the actor
    /// was already exiting or dead.
    fn signal(&self, signal: Signal) -> impl Future<Output = Result<bool, ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(local.signal(signal)),
                Target::Remote { .. } => address.call_op(SignalOp(signal)).await,
            }
        }
    }

    /// Sends a [`Signal::Shutdown`] to the actor. Returns `false` if it was
    /// already exiting or dead.
    fn signal_shutdown(&self) -> impl Future<Output = Result<bool, ClusterOpError>> + Send {
        self.signal(Signal::Shutdown)
    }

    /// Sends a [`Signal::Suspend`] to the actor. Returns `false` if it was
    /// already exiting or dead.
    fn signal_suspend(&self) -> impl Future<Output = Result<bool, ClusterOpError>> + Send {
        self.signal(Signal::Suspend)
    }

    /// Sends a [`Signal::Resume`] to the actor. Returns `false` if it was
    /// already exiting or dead.
    fn signal_resume(&self) -> impl Future<Output = Result<bool, ClusterOpError>> + Send {
        self.signal(Signal::Resume)
    }

    /// Waits until the actor has processed a signal. As signals are processed
    /// before the messages queued, this confirms that the actor is alive and
    /// its event loop has caught up with its signals.
    fn ping(&self) -> impl Future<Output = Result<(), ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => local.ping().await.map_err(|_| {
                    ClusterOpError::Reply(ClusterReplyError::Remote(RemoteError::NoReply))
                }),
                Target::Remote { .. } => address.call_op(PingOp).await,
            }
        }
    }

    /// Asks about the actor's state, everything at one instant. This is the
    /// only operation that carries the actor's history and accepts list; the
    /// methods below that read one field ask for that field alone.
    fn info(&self) -> impl Future<Output = Result<ActorInfo, ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(ActorInfo {
                    snapshot: local.snapshot(),
                    reached_backpressure: local.reached_backpressure(),
                    accepts: address.cluster().accepted_ids(local.as_dyn()),
                }),
                Target::Remote { .. } => address.call_op(InfoOp).await,
            }
        }
    }

    /// Captures a [`ChannelSnapshot`] of the actor. Its timestamps are from the
    /// clock of the actor's node.
    fn snapshot(&self) -> impl Future<Output = Result<ChannelSnapshot, ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(local.snapshot()),
                Target::Remote { .. } => Ok(address.info().await?.snapshot),
            }
        }
    }

    /// The actor's current [`ActorStatus`].
    fn status(&self) -> impl Future<Output = Result<ActorStatus, ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(local.status()),
                Target::Remote { .. } => Ok(address.call_op(StateOp).await?.status),
            }
        }
    }

    /// Whether the actor's status is [`ActorStatus::Exiting`].
    fn is_exiting(&self) -> impl Future<Output = Result<bool, ClusterOpError>> + Send {
        async move { Ok(self.status().await?.is_exiting()) }
    }

    /// Whether the actor's status is [`ActorStatus::Exited`].
    fn is_dead(&self) -> impl Future<Output = Result<bool, ClusterOpError>> + Send {
        async move { Ok(self.status().await?.is_dead()) }
    }

    /// The number of messages currently queued for the actor.
    fn msg_len(&self) -> impl Future<Output = Result<usize, ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(local.msg_len()),
                Target::Remote { .. } => Ok(address.call_op(StateOp).await?.msg_len),
            }
        }
    }

    /// Whether no messages are currently queued for the actor.
    fn msg_is_empty(&self) -> impl Future<Output = Result<bool, ClusterOpError>> + Send {
        async move { Ok(self.msg_len().await? == 0) }
    }

    /// The number of signals currently queued for the actor.
    fn signal_len(&self) -> impl Future<Output = Result<usize, ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(local.signal_len()),
                Target::Remote { .. } => Ok(address.call_op(StateOp).await?.signal_len),
            }
        }
    }

    /// Whether no signals are currently queued for the actor.
    fn signal_is_empty(&self) -> impl Future<Output = Result<bool, ClusterOpError>> + Send {
        async move { Ok(self.signal_len().await? == 0) }
    }

    /// Whether the actor's mailbox is full, so that sending to it waits.
    fn reached_backpressure(&self) -> impl Future<Output = Result<bool, ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(local.reached_backpressure()),
                Target::Remote { .. } => Ok(address.call_op(StateOp).await?.reached_backpressure),
            }
        }
    }

    /// When the actor was last spawned, on the clock of its node, or `None` if
    /// it never was.
    fn last_spawned_at(
        &self,
    ) -> impl Future<Output = Result<Option<Zoned>, ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(local.snapshot().spawns.last().cloned()),
                Target::Remote { .. } => Ok(address.info().await?.snapshot.spawns.last().cloned()),
            }
        }
    }

    /// The ids of the message types the actor accepts and that its node has
    /// registered, see [`ActorInfo::accepts`].
    fn members(&self) -> impl Future<Output = Result<Vec<MessageId>, ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(address.cluster().accepted_ids(local.as_dyn())),
                Target::Remote { .. } => Ok(address.info().await?.accepts),
            }
        }
    }

    /// Whether the actor accepts messages of type `M`. For a remote actor,
    /// `false` also if its node hasn't registered it.
    fn accepts<M: RemoteMessage>(
        &self,
    ) -> impl Future<Output = Result<bool, ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(local.accepts::<M>()),
                Target::Remote { .. } => self.accepts_id(M::Id).await,
            }
        }
    }

    /// Whether the actor accepts messages with the id `id`, see
    /// [`ClusterActorOps::accepts`].
    fn accepts_id(
        &self,
        id: MessageId,
    ) -> impl Future<Output = Result<bool, ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(address.cluster().accepts_ids(local.as_dyn(), &[id])),
                // One question, rather than the node's whole list back to scan.
                Target::Remote { .. } => address.call_op(AcceptsOp(vec![id])).await,
            }
        }
    }

    /// Whether the actor accepts every message type in `ids`.
    fn is_superset_of<'a>(
        &'a self,
        ids: &'a [MessageId],
    ) -> impl Future<Output = Result<bool, ClusterOpError>> + Send + 'a {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(address.cluster().accepts_ids(local.as_dyn(), ids)),
                Target::Remote { .. } => address.call_op(AcceptsOp(ids.to_vec())).await,
            }
        }
    }

    /// Waits until the actor's status is one of `kinds`, and returns it — with
    /// the exit reason, if it exited. The counterpart of
    /// [`ActorOps::monitor_any`](zestors_runtime::ActorOps::monitor_any).
    ///
    /// The status the actor is already in counts, so this can return at once.
    /// For an actor on another node the monitor is held there until it is
    /// reached, with no deadline; losing the node ends it with
    /// [`ClusterReplyError::Disconnected`], which is the only answer there is.
    ///
    /// Never returns if `kinds` is empty, or if the actor never reaches any of
    /// them and its node stays up; include [`ActorStatusKind::Exited`] to be
    /// sure of an answer.
    fn monitor_any(
        &self,
        kinds: &[ActorStatusKind],
    ) -> impl Future<Output = Result<ActorStatus, ClusterOpError>> + Send {
        async move {
            let address = self.cluster_address();
            match address.target() {
                Target::Local(local) => Ok(local.monitor_any(kinds).await),
                Target::Remote { .. } => address.monitor_remote(kinds).await,
            }
        }
    }

    /// Waits until the actor has exited, and reports how.
    fn monitor_exit(
        &self,
    ) -> impl Future<Output = Result<Result<(), ExitError>, ClusterOpError>> + Send {
        async move {
            match self.monitor_any(&[ActorStatusKind::Exited]).await? {
                ActorStatus::Exited(exit) => Ok(exit.into_result()),
                status => unreachable!("Monitored for an exit, got {status:?}"),
            }
        }
    }

    /// Waits until the actor is running.
    fn monitor_running(&self) -> impl Future<Output = Result<(), ClusterOpError>> + Send {
        async move {
            self.monitor_any(&[ActorStatusKind::Running]).await?;
            Ok(())
        }
    }

    /// Waits until the actor takes signals and messages, see
    /// [`ActorStatus::accepts_messages`]. Never returns for an actor that exits
    /// without ever accepting one.
    fn monitor_accepts_messages(&self) -> impl Future<Output = Result<(), ClusterOpError>> + Send {
        async move {
            self.monitor_any(&[
                ActorStatusKind::Initializing,
                ActorStatusKind::Running,
                ActorStatusKind::Suspended,
            ])
            .await?;
            Ok(())
        }
    }

    /// Waits until the actor is running, or `Err` with how it exited if it got
    /// there first.
    ///
    /// Not quite [`ActorOps::monitor_init`](zestors_runtime::ActorOps::monitor_init):
    /// that one also checks whether the actor was ever spawned, to tell a
    /// channel that was made but never run from one that really exited. That
    /// check can't be asked of another node, so a never-spawned actor reads as
    /// exited here. Reaching one by name makes that a corner rather than the
    /// usual case.
    fn monitor_init(
        &self,
    ) -> impl Future<Output = Result<Result<(), ExitStatus>, ClusterOpError>> + Send {
        async move {
            match self
                .monitor_any(&[ActorStatusKind::Running, ActorStatusKind::Exited])
                .await?
            {
                ActorStatus::Exited(exit) => Ok(Err(exit)),
                _ => Ok(Ok(())),
            }
        }
    }

    /// The actor's name.
    fn name(&self) -> &Name {
        match self.cluster_address().target() {
            Target::Local(local) => local.name(),
            Target::Remote { name, .. } => name.name(),
        }
    }
}

impl<T: ClusterActorRef> ClusterActorOps for T {}

mod sealed {
    pub trait Sealed {}
    impl<T: super::ClusterActorRef> Sealed for T {}
}
