use super::*;
use crate::signals;
use jiff::{SignedDuration, Timestamp, Zoned, tz::TimeZone};
use std::{any::TypeId, future::Future};
use tokio::time::Instant;
use zestors_interface::{Reply, Request};

/// A trait that provides access to the [`Channel`] of an actor.
///
/// Implement this trait, and [`ActorOps`] is automatically implemented for your
/// type.
pub trait ActorRef {
    /// The [`Context`] of the associated actor.
    type Ctx: Context;

    /// Returns a reference to the [`Channel`] of the associated actor.
    fn channel(&self) -> &Channel<Self::Ctx>;
}

/// The core trait for interacting with actors through their [`Channel`].
/// This trait is sealed, and is implemented automatically for any type that
/// implements [`ActorRef`].
pub trait ActorOps: ActorRef + sealed::Sealed {
    /// Same as [`Cast::cast`], but works for any actor reference regardless of
    /// whether its [`Context`] statically guarantees that `M` is accepted:
    /// the check is performed at runtime instead, returning
    /// [`CastDynError::NotAccepted`] rather than failing to compile.
    fn cast_dyn<M: Message>(
        &self,
        msg: M,
    ) -> impl Future<Output = Result<M::Receipt, CastDynError<M>>> + Send {
        self.cast_dyn_with(msg, Default::default())
    }

    /// Same as [`Cast::cast_with`], but see [`ActorOps::cast_dyn`] for how it
    /// differs from [`Cast::cast_with`].
    fn cast_dyn_with<M: Message>(
        &self,
        msg: M,
        mut options: CastOptions,
    ) -> impl Future<Output = Result<M::Receipt, CastDynError<M>>> + Send {
        let handle = self.channel();

        async move {
            if !options.ignore_backpressure {
                handle.delay_for_backpressure().await;
                options.ignore_backpressure = true;
            }

            handle
                .try_cast_dyn_with(msg, options)
                .map_err(|e| e.into_cast_error_dbg_assert())
        }
    }

    /// Same as [`Cast::try_cast`], but see [`ActorOps::cast_dyn`] for how it
    /// differs from [`Cast::cast`].
    fn try_cast_dyn<M: Message>(&self, msg: M) -> Result<M::Receipt, TryCastDynError<M>> {
        self.try_cast_dyn_with(msg, Default::default())
    }

    /// Same as [`Cast::try_cast_with`], but checks at runtime whether `M` is
    /// accepted by the channel, returning [`TryCastDynError::NotAccepted`]
    /// rather than failing to compile.
    fn try_cast_dyn_with<M: Message>(
        &self,
        msg: M,
        options: CastOptions,
    ) -> Result<M::Receipt, TryCastDynError<M>> {
        if !options.ignore_backpressure && self.reached_backpressure() {
            return Err(TryCastDynError::Full(msg));
        }

        let status = self.status();
        if !status.accepts_messages() && !(options.ignore_exiting && status.is_exiting()) {
            return Err(TryCastDynError::Closed(msg));
        }

        let output = self.channel().try_push_msg(msg)?;
        self.channel().msg_notify_one();
        Ok(output)
    }

    /// Same as [`Cast::call`], but see [`ActorOps::cast_dyn`] for how it
    /// differs from [`Cast::cast`].
    fn call_dyn<M: Message>(
        &self,
        msg: M,
    ) -> impl Future<Output = Result<M::Output, CallDynError<M>>> + Send {
        self.call_dyn_with(msg, Default::default())
    }

    /// Same as [`Cast::call_with`], but see [`ActorOps::cast_dyn`] for how it
    /// differs from [`Cast::cast_with`].
    fn call_dyn_with<M: Message>(
        &self,
        msg: M,
        options: CastOptions,
    ) -> impl Future<Output = Result<M::Output, CallDynError<M>>> + Send {
        let handle = self.channel();
        async move { Ok(handle.cast_dyn_with(msg, options).await?.wait().await?) }
    }

    /// Returns the actor's [`Pid`].
    fn pid(&self) -> &Pid {
        self.data().pid()
    }

    /// Returns the actor's current [`ActorStatus`].
    fn status(&self) -> ActorStatus {
        self.data().status()
    }

    /// Returns `true` if the actor's status is [`ActorStatus::Exiting`].
    fn is_exiting(&self) -> bool {
        self.status().is_exiting()
    }

    /// Captures a [`ChannelSnapshot`] of the actor's current status, queue
    /// lengths, and spawn/exit history.
    fn snapshot(&self) -> ChannelSnapshot {
        let clock = Clock::now();
        let data = &self.data();

        ChannelSnapshot {
            pid: data.pid().clone(),
            status: data.status(),
            signal_len: data.signal_len(),
            msg_len: data.msg_len(),
            spawns: data
                .spawned_at()
                .into_iter()
                .map(|instant| clock.zoned_at(instant))
                .collect(),
            exits: data
                .exits()
                .into_iter()
                .map(|(instant, res)| (clock.zoned_at(instant), ExitStatus::from_result(res)))
                .collect(),
            created_at: clock.zoned_at(data.created_at()),
        }
    }

    /// Waits until `check_for` returns `Some` for the actor's [`ActorStatus`].
    /// Checked once against the current status, and then again after every
    /// subsequent status change, until `check_for` returns `Some`.
    fn watch<T>(
        &self,
        check_for: impl FnMut(ActorStatus) -> Option<T> + Send + 'static,
    ) -> impl Future<Output = T> + Send {
        self.data().watch(check_for)
    }

    /// Waits until the actor reaches [`ActorStatus::Running`] for the first
    /// time, returning `Err` with the actor's [`ExitStatus`] if it instead
    /// reaches [`ActorStatus::Exited`] beforehand.
    fn watch_init(&self) -> impl Future<Output = Result<(), ExitStatus>> + Send {
        self.watch(|status| match status {
            ActorStatus::Running => return Some(Ok(())),
            ActorStatus::Exited(exit) => {
                return Some(Err(exit));
            }
            _ => None,
        })
    }

    /// Waits until the actor reaches [`ActorStatus::Exited`], returning the
    /// outcome as a `Result`.
    fn watch_exit(&self) -> impl Future<Output = Result<(), ExitError>> + Send {
        self.watch(|status| match status {
            ActorStatus::Exited(exit) => return Some(exit.into_result()),
            _ => None,
        })
    }

    /// Returns the [`TypeId`]s of every message type the channel accepts.
    fn members(&self) -> &'static [TypeId] {
        self.data().members()
    }

    /// Returns the number of messages currently queued.
    fn msg_len(&self) -> usize {
        self.data().msg_len()
    }

    /// Returns `true` if there are no messages currently queued.
    fn msg_is_empty(&self) -> bool {
        self.msg_len() == 0
    }

    /// Returns the number of signals currently queued.
    fn signal_len(&self) -> usize {
        self.data().signal_len()
    }

    /// Returns `true` if there are no signals currently queued.
    fn signal_is_empty(&self) -> bool {
        self.signal_len() == 0
    }

    /// Returns `true` if the channel accepts messages of type `M`.
    fn can_send<M: Message>(&self) -> bool {
        self.can_send_type_id(TypeId::of::<M>())
    }

    /// The [`TypeId`]-based primitive behind [`ActorOps::can_send`], for use
    /// where the message type isn't statically known. See
    /// [`ActorOps::is_superset_of`].
    fn can_send_type_id(&self, type_id: TypeId) -> bool {
        self.members().contains(&type_id)
    }

    /// Returns `true` if the channel accepts every message type in
    /// `type_ids`. Used internally by [`IntoDyn::into_dyn_checked`] and
    /// [`AsDyn::as_dyn_checked`].
    fn is_superset_of(&self, type_ids: &[TypeId]) -> bool {
        type_ids.iter().all(|id| self.can_send_type_id(*id))
    }

    /// Returns `true` if the channel's concrete message type is exactly `I`.
    fn is_interface<I: Interface>(&self) -> bool {
        self.data().is_interface::<I>()
    }

    /// Returns `true` if the channel is currently under backpressure, i.e.
    /// sending would incur a delay (via [`Cast::cast`]) or fail with
    /// [`TryCastError::Full`] (via [`Cast::try_cast`]).
    fn reached_backpressure(&self) -> bool {
        let handle = self.channel();

        handle
            .backpressure()
            .delay(handle.data().msg_len(), handle.data().backpressure_limit())
            .is_some()
    }

    /// Sends a [`Signal::Shutdown`] to the actor. Returns `false` if the
    /// channel was already exiting or dead.
    fn signal_shutdown(&self) -> bool {
        self.signal(Signal::Shutdown)
    }

    /// Sends a [`Signal::Suspend`] to the actor. Returns `false` if the
    /// channel was already exiting or dead.
    fn signal_suspend(&self) -> bool {
        self.signal(Signal::Suspend)
    }

    /// Sends a [`Signal::Resume`] to the actor. Returns `false` if the
    /// channel was already exiting or dead.
    fn signal_resume(&self) -> bool {
        self.signal(Signal::Resume)
    }

    /// Sends the given [`Signal`] to the actor. Returns `false` if the
    /// channel was already exiting or dead.
    fn signal(&self, signal: Signal) -> bool {
        let interface = match signal {
            Signal::Shutdown => SignalInterface::Shutdown(Envelope::new(signals::Shutdown, ())),
            Signal::Suspend => SignalInterface::Suspend(Envelope::new(signals::Suspend, ())),
            Signal::Resume => SignalInterface::Resume(Envelope::new(signals::Resume, ())),
        };

        self.data().signal(interface)
    }

    /// Sends a liveness-check signal to the actor, returning a [`Reply`] that
    /// resolves once the actor's event loop has processed it. Since signals
    /// are always processed before queued messages, this can be used to
    /// confirm the actor has caught up to this point in its signal queue.
    fn ping(&self) -> Reply<()> {
        let (tx, rx) = Request::new();

        self.channel()
            .data()
            .signal(SignalInterface::Ping(Envelope::new(signals::Ping, tx)));

        rx
    }

    /// Returns the [`Instant`] at which the channel was created. This is
    /// fixed for the channel's lifetime; see [`ActorOps::last_spawned_at`]
    /// for the most recent spawn.
    fn created_at(&self) -> Instant {
        self.data().created_at()
    }

    /// Returns the [`Instant`] of the most recent spawn, or `None` if the
    /// actor has never been spawned.
    fn last_spawned_at(&self) -> Option<Instant> {
        self.data().last_spawned_at()
    }

    /// Returns the [`Instant`]s of the most recent spawns, oldest first. This
    /// is a bounded history: older entries are dropped once the limit is
    /// reached.
    fn spawned_at(&self) -> Vec<Instant> {
        self.data().spawned_at()
    }

    /// Returns the time elapsed since the actor's most recent spawn, or
    /// `None` if it has never been spawned.
    ///
    /// This keeps counting after the actor has exited — it measures time
    /// since the last spawn, not how long the actor was running for. Check
    /// [`ActorOps::is_dead`] if you need to know whether it's still running.
    fn uptime(&self) -> Option<Duration> {
        self.last_spawned_at().map(|instant| instant.elapsed())
    }

    /// Returns `true` if the actor's status is [`ActorStatus::Exited`].
    fn is_dead(&self) -> bool {
        self.status().is_dead()
    }

    /// Returns `true` if the actor is dead and no [`StrongAddress`],
    /// [`Inbox`] or [`Child`] reference remains that could respawn it.
    fn is_permanently_dead(&self) -> bool {
        self.status().is_dead() && self.strong_count() == 0
    }

    /// The number of [`StrongAddress`]es currently alive for this channel —
    /// including the ones held internally by every [`Inbox`] and [`Child`].
    /// Once this reaches zero, the channel is permanently dead (see
    /// [`ActorOps::is_permanently_dead`]).
    ///
    /// This amount should only be used as an indication of the number of
    /// active references to the channel.
    fn strong_count(&self) -> usize {
        self.data().strong_count()
    }

    /// The total number of live references to this channel, both strong
    /// ([`StrongAddress`], and the ones held internally by [`Inbox`]/[`Child`])
    /// and weak ([`Address`]).
    ///
    /// This amount should only be used as an indication of the number of
    /// active references to the channel.
    fn ref_count(&self) -> usize {
        self.channel().ref_count()
    }

    /// The amount of [`Address`]es in existence for this channel.
    ///
    /// This amount should only be used as an indication of the number of
    /// active references to the channel.
    fn weak_count(&self) -> usize {
        self.ref_count().saturating_sub(self.strong_count())
    }

    /// Returns a weak [`Address`] reference to the channel, borrowed from
    /// `self`.
    fn address(&self) -> &Address<Self::Ctx> {
        Address::from_ref(self.channel())
    }

    /// Attempts to obtain a [`StrongAddress`] to the channel, returning
    /// `None` if it is permanently dead (see [`ActorOps::is_permanently_dead`]).
    fn upgrade(&self) -> Option<StrongAddress<Self::Ctx>> {
        StrongAddress::from_channel_ref(self.channel())
    }
}

impl<T: ActorRef> ActorOps for T {}

#[derive(Clone, Copy)]
struct Clock {
    instant: Instant,
    timestamp: Timestamp,
}

impl Clock {
    fn now() -> Self {
        Self {
            instant: Instant::now(),
            timestamp: Timestamp::now(),
        }
    }

    fn timestamp_at(self, instant: Instant) -> Timestamp {
        let elapsed = instant.duration_since(self.instant);

        self.timestamp + SignedDuration::from_nanos(elapsed.as_nanos() as i64)
    }

    fn zoned_at(self, instant: Instant) -> Zoned {
        self.timestamp_at(instant).to_zoned(TimeZone::UTC)
    }
}

trait ChannelAccess: ActorRef {
    fn data(&self) -> &ChannelInner<dyn DynamicQueue> {
        self.channel().data()
    }
}
impl<T: ActorRef + ?Sized> ChannelAccess for T {}

mod sealed {
    pub trait Sealed {}
    impl<T: super::ActorRef> Sealed for T {}
}

/// Options controlling how a [`Cast`]/[`ActorOps`] sending method behaves.
/// The default, used by [`Cast::cast`]/[`Cast::try_cast`]/etc., disables both
/// options below.
#[derive(Debug, Clone, Copy)]
pub struct CastOptions {
    /// If `true`, a message is still accepted while the channel is
    /// [`ActorStatus::Exiting`]. Has no effect once the channel is fully
    /// [`ActorStatus::Exited`], which always rejects new messages.
    pub ignore_exiting: bool,
    /// If `true`, backpressure is ignored entirely: waiting methods (e.g.
    /// [`Cast::cast`]) skip their delay, and non-waiting methods (e.g.
    /// [`Cast::try_cast`]) skip the check that would otherwise return a
    /// full-channel error.
    pub ignore_backpressure: bool,
}

impl Default for CastOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl CastOptions {
    /// Creates a new [`CastOptions`] with both options disabled.
    pub fn new() -> Self {
        Self {
            ignore_exiting: false,
            ignore_backpressure: false,
        }
    }

    /// Sets [`CastOptions::ignore_exiting`].
    pub fn ignore_exiting(mut self, ignore: bool) -> Self {
        self.ignore_exiting = ignore;
        self
    }

    /// Sets [`CastOptions::ignore_backpressure`].
    pub fn ignore_backpressure(mut self, ignore: bool) -> Self {
        self.ignore_backpressure = ignore;
        self
    }
}
