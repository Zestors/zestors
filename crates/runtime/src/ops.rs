use super::*;
use crate::signals;
use jiff::{SignedDuration, Timestamp, Zoned, tz::TimeZone};
use std::{any::TypeId, future::Future, sync::Arc};
use tokio::time::Instant;
use zestors_interface::{Reply, Request};

/// A trait that provides access to the [`Address`] of an actor.
///
/// Implement this trait, and [`ActorOps`] is automatically implemented for your
/// type.
pub trait ActorRef {
    /// The [`Context`] of the associated actor.
    type Ctx: Context;

    /// Returns a reference to the [`Address`] of the associated actor. See
    /// [`ActorOps::address`] for the same thing, re-exposed on the trait
    /// that's part of the [`prelude`](crate::prelude).
    fn actor_ref(&self) -> &Address<Self::Ctx>;
}

/// The core trait for interacting with actors through their [`Address`].
/// This trait is sealed, and is implemented automatically for any type that
/// implements [`ActorRef`].
pub trait ActorOps: ActorRef + sealed::Sealed {
    /// Same as [`Accepts::cast`], but works for any actor reference regardless of
    /// whether its [`Context`] statically guarantees that `M` is accepted:
    /// the check is performed at runtime instead, returning
    /// [`CastDynError::NotAccepted`] rather than failing to compile.
    fn cast_dyn<M: Message>(
        &self,
        msg: M,
    ) -> impl Future<Output = Result<M::Receipt, CastDynError<M>>> + Send {
        self.cast_dyn_with(msg, Default::default())
    }

    /// Same as [`Accepts::cast_with`], but see [`ActorOps::cast_dyn`] for how it
    /// differs from [`Accepts::cast_with`].
    fn cast_dyn_with<M: Message>(
        &self,
        msg: M,
        options: CallOptions,
    ) -> impl Future<Output = Result<M::Receipt, CastDynError<M>>> + Send {
        self.channel().cast_dyn_with(msg, options)
    }

    /// Same as [`Accepts::try_cast`], but see [`ActorOps::cast_dyn`] for how it
    /// differs from [`Accepts::cast`].
    fn try_cast_dyn<M: Message>(&self, msg: M) -> Result<M::Receipt, TryCastDynError<M>> {
        self.try_cast_dyn_with(msg, Default::default())
    }

    /// Same as [`Accepts::try_cast_with`], but checks at runtime whether `M` is
    /// accepted by the channel, returning [`TryCastDynError::NotAccepted`]
    /// rather than failing to compile.
    fn try_cast_dyn_with<M: Message>(
        &self,
        msg: M,
        options: CallOptions,
    ) -> Result<M::Receipt, TryCastDynError<M>> {
        self.channel().try_cast_dyn_with(msg, options)
    }

    /// Same as [`Accepts::call`], but see [`ActorOps::cast_dyn`] for how it
    /// differs from [`Accepts::cast`].
    fn call_dyn<M: Message>(
        &self,
        msg: M,
    ) -> impl Future<Output = Result<M::Output, CallDynError<M>>> + Send {
        self.call_dyn_with(msg, Default::default())
    }

    /// Same as [`Accepts::call_with`], but see [`ActorOps::cast_dyn`] for how it
    /// differs from [`Accepts::cast_with`].
    fn call_dyn_with<M: Message>(
        &self,
        msg: M,
        options: CallOptions,
    ) -> impl Future<Output = Result<M::Output, CallDynError<M>>> + Send {
        let channel = self.channel();
        async move { Ok(channel.cast_dyn_with(msg, options).await?.wait().await?) }
    }

    /// Returns the actor's [`Pid`].
    fn pid(&self) -> &Pid {
        self.channel().pid()
    }

    /// Returns the actor's current [`ActorStatus`].
    fn status(&self) -> ActorStatus {
        self.channel().status()
    }

    /// Returns `true` if the actor's status is [`ActorStatus::Exiting`].
    fn is_exiting(&self) -> bool {
        self.status().is_exiting()
    }

    /// Captures a [`ChannelSnapshot`] of the actor's current status, queue
    /// lengths, and spawn/exit history.
    fn snapshot(&self) -> ChannelSnapshot {
        let clock = Clock::now();
        let data = &self.channel();

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
        self.channel().watch(check_for)
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
        self.channel().members()
    }

    /// Returns the number of messages currently queued.
    fn msg_len(&self) -> usize {
        self.channel().msg_len()
    }

    /// Returns `true` if there are no messages currently queued.
    fn msg_is_empty(&self) -> bool {
        self.msg_len() == 0
    }

    /// Returns the number of signals currently queued.
    fn signal_len(&self) -> usize {
        self.channel().signal_len()
    }

    /// Returns `true` if there are no signals currently queued.
    fn signal_is_empty(&self) -> bool {
        self.signal_len() == 0
    }

    /// Returns `true` if the channel accepts messages of type `M`.
    fn accepts<M: Message>(&self) -> bool {
        self.accepts_id(TypeId::of::<M>())
    }

    /// The [`TypeId`]-based primitive behind [`ActorOps::accepts`], for use
    /// where the message type isn't statically known. See
    /// [`ActorOps::is_superset_of`].
    fn accepts_id(&self, type_id: TypeId) -> bool {
        self.members().contains(&type_id)
    }

    /// Returns `true` if the channel accepts every message type in
    /// `type_ids`. Used internally by [`IntoDyn::into_dyn_checked`] and
    /// [`AsDyn::as_dyn_checked`].
    fn is_superset_of(&self, type_ids: &[TypeId]) -> bool {
        type_ids.iter().all(|id| self.accepts_id(*id))
    }

    /// Returns `true` if the channel's concrete message type is exactly `I`.
    fn is_interface<I: Interface>(&self) -> bool {
        self.channel().is_interface::<I>()
    }

    /// Returns `true` if the channel is currently under backpressure, i.e.
    /// sending would incur a delay (via [`Accepts::cast`]) or fail with
    /// [`TryCastError::Full`] (via [`Accepts::try_cast`]).
    fn reached_backpressure(&self) -> bool {
        self.channel().reached_backpressure()
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

        self.channel().signal(interface)
    }

    /// Sends a liveness-check signal to the actor, returning a [`Reply`] that
    /// resolves once the actor's event loop has processed it. Since signals
    /// are always processed before queued messages, this can be used to
    /// confirm the actor has caught up to this point in its signal queue.
    fn ping(&self) -> Reply<()> {
        let (tx, rx) = Request::new();

        self.channel()
            .signal(SignalInterface::Ping(Envelope::new(signals::Ping, tx)));

        rx
    }

    /// Returns the [`Instant`] at which the channel was created. This is
    /// fixed for the channel's lifetime; see [`ActorOps::last_spawned_at`]
    /// for the most recent spawn.
    fn created_at(&self) -> Instant {
        self.channel().created_at()
    }

    /// Returns the [`Instant`] of the most recent spawn, or `None` if the
    /// actor has never been spawned.
    fn last_spawned_at(&self) -> Option<Instant> {
        self.channel().last_spawned_at()
    }

    /// Returns the [`Instant`]s of the most recent spawns, oldest first. This
    /// is a bounded history: older entries are dropped once the limit is
    /// reached.
    fn spawned_at(&self) -> Vec<Instant> {
        self.channel().spawned_at()
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
        self.channel().strong_count()
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
        self.channel().weak_count()
    }

    /// Returns a reference to the [`Address`] of the associated actor. Same
    /// as [`ActorRef::actor_ref`], re-exposed here since [`ActorRef`] itself
    /// is not part of the [`prelude`](crate::prelude).
    fn address(&self) -> &Address<Self::Ctx> {
        self.actor_ref()
    }

    /// Attempts to obtain a [`StrongAddress`] to the channel, returning
    /// `None` if it is permanently dead (see [`ActorOps::is_permanently_dead`]).
    fn upgrade(&self) -> Option<StrongAddress<Self::Ctx>> {
        StrongAddress::from_address_ref(self.address())
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

pub(crate) trait ChannelAccess: ActorRef {
    fn channel(&self) -> &Arc<Channel<dyn DynamicQueue>> {
        self.actor_ref()._channel()
    }
}
impl<T: ActorRef + ?Sized> ChannelAccess for T {}

mod sealed {
    pub trait Sealed {}
    impl<T: super::ActorRef> Sealed for T {}
}
