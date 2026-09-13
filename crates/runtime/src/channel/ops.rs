use super::*;
use crate::signals;
use jiff::{SignedDuration, Timestamp, Zoned, tz::TimeZone};
use std::{any::TypeId, future::Future};
use tokio::time::Instant;
use zestors_interface::{Request, Response};

/// A trait that provides access to the [`ActorHandle`] of an actor.
///
/// Implement this trait, and [`ActorOpsExt`] is automatically implemented for your
/// type.
pub trait ActorRef {
    /// The [`Context`] of the associated actor.
    type Ctx: Context;

    /// Returns a reference to the [`ActorHandle`] of the associated actor.
    fn channel(&self) -> &Channel<Self::Ctx>;
}

/// The core trait for interacting with actors through their [`ActorHandle`].
/// This trait is sealed, and is implemented automatically for any type that
/// implements [`ActorOps`].
pub trait ActorOps: ActorRef + sealed::Sealed {
    /// Same as [`Sends::send`], but checks whether the message type is accepted by the channel.
    fn send_dyn<M: Message>(
        &self,
        msg: M,
    ) -> impl Future<Output = Result<M::Receipt, SendCheckedError<M>>> + Send {
        let handle = self.channel();

        async {
            handle.delay_for_backpressure().await;
            handle.send_now_dyn(msg)
        }
    }

    /// Same as [`Sends::try_send`], but checks whether the message type is accepted by the channel.
    fn try_send_dyn<M: Message>(&self, msg: M) -> Result<M::Receipt, TrySendCheckedError<M>> {
        if self.reached_backpressure() {
            return Err(TrySendCheckedError::Full(msg));
        }

        self.send_now_dyn(msg).map_err(Into::into)
    }

    /// Same as [`Sends::send_now`], but checks whether the message type is accepted by the channel.
    fn send_now_dyn<M: Message>(&self, msg: M) -> Result<M::Receipt, SendCheckedError<M>> {
        if !self.status().accepts_messages() {
            return Err(SendCheckedError::Closed(msg));
        }

        let output = self.channel().try_push_msg(msg)?;
        self.channel().msg_notify_one();
        Ok(output)
    }

    fn request_dyn<M: Message>(
        &self,
        msg: M,
    ) -> impl Future<Output = Result<M::Output, RequestCheckedError<M>>> + Send {
        let handle = self.channel();
        async { Ok(handle.send_dyn(msg).await?.wait().await?) }
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

    fn watch_initialization(&self) -> impl Future<Output = Result<(), ExitStatus>> + Send {
        self.watch(|status| match status {
            ActorStatus::Running => return Some(Ok(())),
            ActorStatus::Exited(exit) => {
                return Some(Err(exit));
            }
            _ => None,
        })
    }

    fn watch_start(&self) -> impl Future<Output = ()> + Send
    where
        Self: Sync,
    {
        self.watch(|status| match status {
            ActorStatus::Running => return Some(()),
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

    fn ping(&self) -> Response<()> {
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

    /// The total amount of references to this channel, including [`ChannelHandle`]s, [`Inbox`]es and [`Address`]es.
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
