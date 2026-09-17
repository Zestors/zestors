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
        if !status.accepts_messages() && !(options.ignore_exiting && status.is_shutting_down()) {
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
    ) -> impl Future<Output = Result<M::Output, CallCheckedError<M>>> + Send {
        self.call_dyn_with(msg, Default::default())
    }

    /// Same as [`Cast::call_with`], but see [`ActorOps::cast_dyn`] for how it
    /// differs from [`Cast::cast_with`].
    fn call_dyn_with<M: Message>(
        &self,
        msg: M,
        options: CastOptions,
    ) -> impl Future<Output = Result<M::Output, CallCheckedError<M>>> + Send {
        let handle = self.channel();
        async move { Ok(handle.cast_dyn_with(msg, options).await?.wait().await?) }
    }

    fn pid(&self) -> &Pid {
        self.data().pid()
    }

    fn status(&self) -> ActorStatus {
        self.data().status()
    }

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

    fn watch<T>(
        &self,
        check_for: impl FnMut(ActorStatus) -> Option<T> + Send + 'static,
    ) -> impl Future<Output = T> + Send {
        self.data().watch(check_for)
    }

    fn watch_init(&self) -> impl Future<Output = Result<(), ExitStatus>> + Send {
        self.watch(|status| match status {
            ActorStatus::Running => return Some(Ok(())),
            ActorStatus::Exited(exit) => {
                return Some(Err(exit));
            }
            _ => None,
        })
    }

    fn watch_exit(&self) -> impl Future<Output = Result<(), ExitError>> + Send {
        self.watch(|status| match status {
            ActorStatus::Exited(exit) => return Some(exit.into_result()),
            _ => None,
        })
    }

    fn members(&self) -> &'static [TypeId] {
        self.data().members()
    }

    fn msg_len(&self) -> usize {
        self.data().msg_len()
    }

    fn msgs_is_empty(&self) -> bool {
        self.msg_len() == 0
    }

    fn can_send(&self, type_id: TypeId) -> bool {
        self.members().contains(&type_id)
    }

    fn is_superset_of(&self, type_ids: &[TypeId]) -> bool {
        type_ids.iter().all(|id| self.can_send(*id))
    }

    fn is_interface<I: Interface>(&self) -> bool {
        self.data().is_interface::<I>()
    }

    fn reached_backpressure(&self) -> bool {
        let handle = self.channel();

        handle
            .backpressure()
            .delay(handle.data().msg_len(), handle.data().backpressure_limit())
            .is_some()
    }

    fn signal_shutdown(&self) -> bool {
        self.signal(Signal::Shutdown)
    }

    fn signal_suspend(&self) -> bool {
        self.signal(Signal::Suspend)
    }

    fn signal_resume(&self) -> bool {
        self.signal(Signal::Resume)
    }

    fn signal(&self, signal: Signal) -> bool {
        let interface = match signal {
            Signal::Shutdown => SignalInterface::Shutdown(Envelope::new(signals::Shutdown, ())),
            Signal::Suspend => SignalInterface::Suspend(Envelope::new(signals::Suspend, ())),
            Signal::Resume => SignalInterface::Resume(Envelope::new(signals::Resume, ())),
        };

        self.data().signal(interface)
    }

    fn ping(&self) -> Reply<()> {
        let (tx, rx) = Request::new();

        self.channel()
            .data()
            .signal(SignalInterface::Ping(Envelope::new(signals::Ping, tx)));

        rx
    }

    fn created_at(&self) -> Instant {
        self.data().created_at()
    }

    fn last_spawned_at(&self) -> Option<Instant> {
        self.data().last_spawned_at()
    }

    fn spawned_at(&self) -> Vec<Instant> {
        self.data().spawned_at()
    }

    fn uptime(&self) -> Option<Duration> {
        self.last_spawned_at().map(|instant| instant.elapsed())
    }

    fn is_dead(&self) -> bool {
        self.status().is_dead()
    }

    fn is_permanently_dead(&self) -> bool {
        self.status().is_dead() && self.strong_count() == 0
    }

    /// The amount of [`channels`](Channel), [`inboxes`](Inbox) and
    /// [`children`](Child) in existence for this
    /// channel.
    ///
    /// This amount should only be used as an indication of the number of
    /// active references to the channel.
    fn strong_count(&self) -> usize {
        self.data().strong_count()
    }

    /// The total amount of references to this channel, including [`Channel`]s, [`StrongAddress`]es, [`Inbox`]es, [`Child`]ren and [`Address`]es.
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

    fn address(&self) -> &Address<Self::Ctx> {
        Address::from_ref(self.channel())
    }

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

trait ActorOpsExtPriv: ActorRef {
    fn data(&self) -> &ChannelInner<dyn DynamicQueue> {
        self.channel().data()
    }
}
impl<T: ActorRef + ?Sized> ActorOpsExtPriv for T {}

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
