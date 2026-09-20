//! The sending side: [`ClusterAccepts`], and how a message is put on its way,
//! to an actor on another node or on this one.

use super::{
    CastFailure, ClusterActorRef, ClusterAddressRef, ClusterCallError, ClusterCastError,
    ClusterOpError, ClusterReceipt, ClusterReply, Decode, RemoteAddress, RemoteMessage,
    frame::Frame,
    message::RemoteMessageKind,
    ops::{DemonitorOp, MonitorOp},
    receive::Session,
    wire::{Exports, Wire},
};
use crate::link::{Delivery, MAX_MESSAGE_SIZE, Protocol};
use bytes::Bytes;
use std::{
    future::Future,
    hash::{Hash, Hasher},
    sync::atomic::Ordering,
    time::Duration,
};
use tokio::sync::mpsc;
use type_sets::Contains;
use zestors_interface::Message;
use zestors_runtime::{
    ActorOps as _, ActorStatus, ActorStatusKind, Address, CallOptions, Context, Name,
    errors::{CastDynError, TryCastDynError},
};

/// Tells the actor's node to forget a monitor, if this is dropped while one is
/// still live there.
///
/// The [`DemonitorOp`] goes to the same actor as the [`MonitorOp`] did, so it takes
/// the same lane — see [`shard_of`] — and cannot overtake it. Were it to, the
/// monitor would never be called off.
struct Demonitor<'a, C: Context> {
    /// `None` once the monitor has been answered.
    address: Option<&'a RemoteAddress<C>>,
    monitor_id: u64,
}

impl<C: Context> Drop for Demonitor<'_, C> {
    fn drop(&mut self) {
        let Some(address) = self.address else {
            return;
        };
        let (address, monitor_id) = (address.clone(), self.monitor_id);
        tokio::spawn(async move {
            // Nothing to report it to: the caller has already gone.
            let _ = address
                .cast_remote(DemonitorOp { monitor_id }, Default::default())
                .await;
        });
    }
}

/// How long a reply is waited for.
#[derive(Clone, Copy)]
enum Deadline {
    /// The one the call options, the address or the node ask for.
    Default,
    /// However long it takes. Losing the node still ends the wait.
    Never,
}

/// A message ready to be sent, and what to wait on for its reply, if it has one.
type Sending<M> = (Prepared, Option<ClusterReply<<M as Message>::Output>>);

/// A message, encoded and ready to go into the lane to its node.
struct Prepared {
    lane: mpsc::Sender<Bytes>,
    frame: Bytes,
    exports: Exports,
}

impl<C: Context> RemoteAddress<C> {
    /// Sends `msg` to the actor on its node, waiting for room in the lane to it.
    pub(super) async fn cast_remote<M: RemoteMessage>(
        &self,
        msg: M,
        options: ClusterCallOptions,
    ) -> Result<M::ClusterReceipt, ClusterCastError<M>> {
        let (prepared, waiting) = match self.prepare(&msg, options, Deadline::Default) {
            Ok(prepared) => prepared,
            Err(reason) => return Err(reason.with(msg)),
        };
        match prepared.lane.send(prepared.frame).await {
            Ok(()) => {
                prepared.exports.commit();
                Ok(M::remote_receipt(waiting))
            }
            Err(_) => Err(CastFailure::Unreachable.with(msg)),
        }
    }

    /// Like [`RemoteAddress::cast_remote`], but fails if the lane is full.
    pub(super) fn try_cast_remote<M: RemoteMessage>(
        &self,
        msg: M,
        options: ClusterCallOptions,
    ) -> Result<M::ClusterReceipt, ClusterCastError<M>> {
        let (prepared, waiting) = match self.prepare(&msg, options, Deadline::Default) {
            Ok(prepared) => prepared,
            Err(reason) => return Err(reason.with(msg)),
        };
        match prepared.lane.try_send(prepared.frame) {
            Ok(()) => {
                prepared.exports.commit();
                Ok(M::remote_receipt(waiting))
            }
            Err(mpsc::error::TrySendError::Full(_)) => Err(CastFailure::Full.with(msg)),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(CastFailure::Unreachable.with(msg)),
        }
    }

    /// Calls one of the [operations](super::ops) on the actor, whatever the
    /// actor accepts.
    pub(super) async fn call_op<M: RemoteMessage>(
        &self,
        msg: M,
    ) -> Result<M::Output, ClusterOpError> {
        self.call_op_with(msg, Deadline::Default).await
    }

    /// Monitors the actor until its status is one of `kinds`.
    ///
    /// The monitor lives on the actor's node until then, so it has to be called
    /// off if this future is dropped — see [`Demonitor`]. It is sent with no
    /// deadline; the node going away is what ends it otherwise.
    pub(super) async fn monitor_remote(
        &self,
        kinds: &[ActorStatusKind],
    ) -> Result<ActorStatus, ClusterOpError> {
        let monitor_id = self
            .cluster
            .messaging()
            .next_call
            .fetch_add(1, Ordering::Relaxed);
        // Armed before the monitor is sent: dropping this future at any point
        // from here on tells the other node to forget it.
        let mut demonitor = Demonitor {
            address: Some(self),
            monitor_id,
        };
        let reached = self
            .call_op_untimed(MonitorOp {
                monitor_id,
                kinds: kinds.to_vec(),
            })
            .await;
        // Answered, so there is nothing left there to call off.
        demonitor.address = None;
        reached
    }

    /// Like [`RemoteAddress::call_op`], for an operation that is answered when
    /// it is answered: a monitor waits for as long as the actor takes.
    pub(super) async fn call_op_untimed<M: RemoteMessage>(
        &self,
        msg: M,
    ) -> Result<M::Output, ClusterOpError> {
        self.call_op_with(msg, Deadline::Never).await
    }

    async fn call_op_with<M: RemoteMessage>(
        &self,
        msg: M,
        deadline: Deadline,
    ) -> Result<M::Output, ClusterOpError> {
        let (prepared, waiting) = match self.prepare(&msg, Default::default(), deadline) {
            Ok(prepared) => prepared,
            Err(reason) => return Err(reason.into()),
        };
        if prepared.lane.send(prepared.frame).await.is_err() {
            return Err(CastFailure::Unreachable.into());
        }
        prepared.exports.commit();
        Ok(M::remote_receipt(waiting).wait().await?)
    }

    /// Encodes `msg` and finds the lane to the node, as a call if the message
    /// expects a reply, else as a cast. For a call, what to wait on for the
    /// reply is set up, so that a reply can't beat it.
    fn prepare<M: RemoteMessage>(
        &self,
        msg: &M,
        options: ClusterCallOptions,
        deadline: Deadline,
    ) -> Result<Sending<M>, CastFailure> {
        let cluster = &self.cluster;
        let messaging = cluster.messaging();
        let running = messaging.running().ok_or(CastFailure::NotRunning)?;
        let member = cluster
            .member(self.target.node())
            .ok_or(CastFailure::NotAMember)?;
        if !cluster.is_reachable(self.target.node()) {
            return Err(CastFailure::Unreachable);
        }

        let session = Session::new(self.cluster.clone(), running.clone());
        let wire = Wire::new(session, member.name.clone());
        let payload = wire.scope(|| msg.encode());
        // Forgets the requests in the message again if it isn't sent.
        let exports = wire.exports();
        let payload = payload.map_err(CastFailure::Encode)?;
        let (target, id) = (self.target.name().clone(), M::Id);
        let call_id = <M::Kind as RemoteMessageKind<M::Output>>::REPLIES
            .then(|| messaging.next_call.fetch_add(1, Ordering::Relaxed));
        let frame = Frame::Message {
            call_id,
            target,
            msg: id,
            requests: exports.ids(),
            payload,
        }
        .encode();

        if frame.len() > MAX_MESSAGE_SIZE {
            return Err(CastFailure::TooLarge {
                size: frame.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }

        let waiting = call_id.map(|call_id| {
            let timeout = match deadline {
                Deadline::Default => Some(
                    options
                        .timeout
                        .or(self.timeout)
                        .unwrap_or(messaging.call_timeout),
                ),
                Deadline::Never => None,
            };
            running.pending.expect(
                call_id,
                member.name.clone(),
                timeout,
                <M::Output as Decode>::decode,
            )
        });

        let lane = running.links.sender(
            &member.name,
            &member.addr,
            Protocol::ACTORS,
            Delivery::Ordered,
            shard_of(self.target.name(), messaging.shards),
        );
        Ok((
            Prepared {
                lane,
                frame,
                exports,
            },
            waiting,
        ))
    }
}

/// Which of a peer's lanes carries messages for `name`. The same one every time,
/// so that messages to one actor stay in order, while different actors mostly
/// don't wait for each other.
fn shard_of(name: &Name, shards: u8) -> u8 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    (hasher.finish() % shards as u64) as u8
}

/// Options for a single message sent with [`ClusterAccepts`], the remote
/// counterpart of [`CallOptions`](zestors_runtime::CallOptions).
///
/// ```
/// # use zestors_distr::ClusterCallOptions;
/// # use std::time::Duration;
/// let options = ClusterCallOptions::new().timeout(Duration::from_secs(2));
/// assert_eq!(options.timeout, Some(Duration::from_secs(2)));
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct ClusterCallOptions {
    /// How long to wait for the reply, instead of the address's or the node's
    /// default.
    pub timeout: Option<Duration>,
}

impl ClusterCallOptions {
    /// Creates a new [`ClusterCallOptions`] with nothing set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets [`ClusterCallOptions::timeout`].
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// Message-sending operations for a reference to an actor anywhere in the
/// cluster: the counterpart of [`Accepts`](zestors_runtime::Accepts), and it
/// works the same way.
///
/// It is implemented for every [`ClusterActorRef`](super::ClusterActorRef) — so
/// [`RemoteAddress`](super::RemoteAddress), [`LocalAddress`](super::LocalAddress)
/// and [`ClusterAddress`](super::ClusterAddress) alike — for every
/// [`RemoteMessage`] that `C` accepts, so only messages the actor is expected to
/// take can be sent.
/// Sending returns the message's [`RemoteMessage::ClusterReceipt`]: `()` for a
/// message that expects no reply, and a [`ClusterReply`](super::ClusterReply) to wait for the reply
/// of one that does. [`call`](Self::call) sends and waits for it.
///
/// There are two ways to send:
///
/// - [`cast`](Self::cast) waits for room to send, and so only fails if the
///   message can't be sent at all.
/// - [`try_cast`](Self::try_cast) never waits: it also fails with
///   [`CastFailure::Full`] if many messages are queued for the node.
///
/// Returning means that the message is queued for sending, not that it arrived
/// or was accepted; for that, wait for the reply. Unlike a local message, one
/// can fail on the way: see [`ClusterReplyError`](super::ClusterReplyError). A message that got no answer
/// is not sent again; delivery is at most once.
pub trait ClusterAccepts<M: RemoteMessage>: Sync {
    /// Sends a message, waiting for room to send it if many messages are
    /// queued for the node.
    ///
    /// Equivalent to [`ClusterAccepts::cast_with`] with the default
    /// [`ClusterCallOptions`].
    fn cast(
        &self,
        msg: M,
    ) -> impl Future<Output = Result<M::ClusterReceipt, ClusterCastError<M>>> + Send {
        self.cast_with(msg, Default::default())
    }

    /// Same as [`ClusterAccepts::cast`], with explicit [`ClusterCallOptions`].
    fn cast_with(
        &self,
        msg: M,
        options: ClusterCallOptions,
    ) -> impl Future<Output = Result<M::ClusterReceipt, ClusterCastError<M>>> + Send;

    /// Sends a message immediately, without waiting for room.
    ///
    /// Equivalent to [`ClusterAccepts::try_cast_with`] with the default
    /// [`ClusterCallOptions`].
    fn try_cast(&self, msg: M) -> Result<M::ClusterReceipt, ClusterCastError<M>> {
        self.try_cast_with(msg, Default::default())
    }

    /// Same as [`ClusterAccepts::try_cast`], with explicit [`ClusterCallOptions`].
    ///
    /// Fails with [`CastFailure::Full`] if many messages are queued for the
    /// node.
    fn try_cast_with(
        &self,
        msg: M,
        options: ClusterCallOptions,
    ) -> Result<M::ClusterReceipt, ClusterCastError<M>>;

    /// Sends a message via [`ClusterAccepts::cast`] and waits for its reply.
    ///
    /// Equivalent to calling [`ClusterAccepts::cast`] and then
    /// [`ClusterReceipt::wait`] on the result, so it shares `cast`'s failures and
    /// adds those of getting the reply. The output is [`Message::Output`](zestors_interface::Message::Output), the
    /// reply. For a message that expects no reply, that is `()` as soon as the
    /// message is queued.
    fn call(&self, msg: M) -> impl Future<Output = Result<M::Output, ClusterCallError<M>>> + Send {
        self.call_with(msg, Default::default())
    }

    /// Same as [`ClusterAccepts::call`], with explicit [`ClusterCallOptions`].
    fn call_with(
        &self,
        msg: M,
        options: ClusterCallOptions,
    ) -> impl Future<Output = Result<M::Output, ClusterCallError<M>>> + Send {
        async move {
            let receipt = self.cast_with(msg, options).await?;
            receipt.wait().await.map_err(ClusterCallError::Reply)
        }
    }
}

/// One implementation for every kind of address: [`LocalAddress`](super::LocalAddress),
/// [`RemoteAddress`](super::RemoteAddress) and [`ClusterAddress`](super::ClusterAddress)
/// are all sent to through it.
impl<M, T> ClusterAccepts<M> for T
where
    M: RemoteMessage,
    T: ClusterActorRef,
    <T::Ctx as Context>::Set: Contains<M>,
{
    async fn cast_with(
        &self,
        msg: M,
        options: ClusterCallOptions,
    ) -> Result<M::ClusterReceipt, ClusterCastError<M>> {
        match self.as_ref() {
            ClusterAddressRef::Remote(address) => address.cast_remote(msg, options).await,
            ClusterAddressRef::Local(local) => cast(local.address(), msg, options).await,
        }
    }

    fn try_cast_with(
        &self,
        msg: M,
        options: ClusterCallOptions,
    ) -> Result<M::ClusterReceipt, ClusterCastError<M>> {
        match self.as_ref() {
            ClusterAddressRef::Remote(address) => address.try_cast_remote(msg, options),
            ClusterAddressRef::Local(local) => try_cast(local.address(), msg, options),
        }
    }
}

/// Sends `msg` to an actor on this node, waiting for room in its mailbox.
pub(super) async fn cast<M: RemoteMessage, C: Context>(
    address: &Address<C>,
    msg: M,
    options: ClusterCallOptions,
) -> Result<M::ClusterReceipt, ClusterCastError<M>> {
    // Not `Accepts::cast`, which panics if a `Dyn` address is for an actor that
    // doesn't accept the message.
    match address.cast_dyn_with(msg, CallOptions::default()).await {
        Ok(receipt) => Ok(M::local_receipt(receipt, options.timeout)),
        Err(CastDynError::Closed(msg)) => Err(CastFailure::Closed.with(msg)),
        Err(CastDynError::NotAccepted(msg)) => Err(CastFailure::NotAccepted.with(msg)),
    }
}

/// Like [`cast`], but fails if the mailbox is full.
pub(super) fn try_cast<M: RemoteMessage, C: Context>(
    address: &Address<C>,
    msg: M,
    options: ClusterCallOptions,
) -> Result<M::ClusterReceipt, ClusterCastError<M>> {
    match address.try_cast_dyn_with(msg, CallOptions::default()) {
        Ok(receipt) => Ok(M::local_receipt(receipt, options.timeout)),
        Err(TryCastDynError::Closed(msg)) => Err(CastFailure::Closed.with(msg)),
        Err(TryCastDynError::Full(msg)) => Err(CastFailure::Full.with(msg)),
        Err(TryCastDynError::NotAccepted(msg)) => Err(CastFailure::NotAccepted.with(msg)),
    }
}
