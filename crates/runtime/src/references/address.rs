use crate::registry::Registry;
use crate::*;
use eyeball::{ObservableWriteGuard, SharedObservable};
use std::{
    any::TypeId,
    convert::Infallible,
    fmt::Debug,
    hash::Hash,
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{select, sync::Notify, time::Instant};

/// A weak reference to an actor's channel, which can be used to send
/// messages and signals to it without keeping it alive.
///
/// Unlike [`StrongAddress`] (and the [`Inbox`]/[`Child`] built on top of it),
/// an `Address` does not count toward the actor's strong reference count:
/// once every strong reference is dropped, the actor is permanently gone even
/// while `Address`es to it still exist. Use [`ActorOps::upgrade`] to attempt
/// to obtain a [`StrongAddress`] from an `Address`.
///
/// Once all strong references to the channel are dropped, the actor is
/// deregistered from the local [`Registry`] and the channel is dropped. This
/// means that in order to restart an actor, a [`StrongAddress`] (or an
/// [`Inbox`]/[`Child`] holding one) must be kept alive.
#[repr(transparent)]
pub struct Address<C: Context = Dyn> {
    inner: Arc<Channel<dyn DynamicQueue>>,
    _ctx: PhantomData<fn() -> C>,
}

impl<C: Context> Address<C> {
    pub(crate) fn _clone(&self) -> Self {
        Self {
            _ctx: PhantomData,
            inner: self.inner.clone(),
        }
    }

    pub(crate) fn decr_strong_count(&self) {
        let prev_count = self.data().strong_count.fetch_sub(1, Ordering::Release);

        // fetch_sub returns the PREVIOUS value. If it was 1, it is now 0.
        if prev_count == 1 {
            // Synchronize memory access from other threads before cleaning up
            std::sync::atomic::fence(Ordering::Acquire);

            let removed_address = Registry::local().remove(self.pid());

            if removed_address.is_none() {
                if cfg!(debug_assertions) {
                    panic!(
                        "Address {} was not found in the registry when dropping the last strong reference",
                        self.pid()
                    );
                } else {
                    tracing::error!(
                        "Address {} was not found in the registry when dropping the last strong reference",
                        self.pid()
                    );
                }
            }
        }
    }

    pub(crate) fn incr_strong_count(&self) {
        // Relaxed is sufficient because the caller already owns a strong reference
        let prev_count = self.data().strong_count.fetch_add(1, Ordering::Relaxed);

        // Prevent integer overflow attack/bug
        if prev_count > usize::MAX / 2 {
            std::process::abort();
        }
    }

    pub(crate) fn data(&self) -> &Channel<dyn DynamicQueue> {
        &self.inner
    }

    pub(crate) fn try_push_msg<M: Message>(&self, msg: M) -> Result<M::Receipt, NotAccepted<M>> {
        self.data().msg_queue.try_push_msg(msg)
    }

    pub(crate) fn msg_notify_one(&self) {
        self.data().msg_notifier.notify_one();
    }

    #[expect(unused)]
    pub(crate) fn pop_dyn(&self) -> Result<AnyEnvelope, PopError> {
        self.data().msg_queue.pop_dyn()
    }

    pub(crate) fn update_status<F, T>(&self, f: F) -> T
    where
        F: FnOnce(ActorStatus) -> (Option<ActorStatus>, T),
    {
        let mut observer = self.data().status_observer.write();
        let prev_status = *observer;

        let (new_status, result) = f(prev_status.clone());

        let Some(new_status) = new_status else {
            return result;
        };

        ObservableWriteGuard::set(&mut observer, new_status);

        result
    }

    pub(crate) fn register_spawned(&self) -> Result<(), InvalidStatusUpdate> {
        tracing::debug!("Process spawned");

        // Spawn is only valid if the actor is dead.
        self.update_status(|status| match status {
            ActorStatus::Exited(_) => (Some(ActorStatus::Initializing), Ok(())),
            ActorStatus::Exiting
            | ActorStatus::Initializing
            | ActorStatus::Running
            | ActorStatus::Suspended => (
                None,
                Err(InvalidStatusUpdate::new(status, ActorStatus::Initializing)),
            ),
        })?;

        let mut spawned_at = self.data().spawns.write().unwrap();

        if spawned_at.len() > KEEP_N_SPAWNS {
            _ = spawned_at.remove(0);
        }

        spawned_at.push(Instant::now());

        Ok(())
    }

    pub(crate) fn register_exited(&self, reason: Result<(), ExitError>) -> bool {
        // Exit is always valid, but doesn't always do something.
        let updated = self.update_status(|status| match status {
            ActorStatus::Exited(_) => (None, false),
            ActorStatus::Exiting
            | ActorStatus::Initializing
            | ActorStatus::Running
            | ActorStatus::Suspended => (
                Some(ActorStatus::Exited(ExitStatus::from_result(reason.clone()))),
                true,
            ),
        });

        match updated {
            true => {
                match &reason {
                    Ok(()) => tracing::debug!("Process exited normally"),
                    Err(err) => tracing::warn!("Process exited with error: {:?}", err),
                }

                let mut exited_at = self.data().exits.write().unwrap();

                if exited_at.len() > KEEP_N_EXITS {
                    _ = exited_at.remove(0);
                }

                exited_at.push((Instant::now(), reason));

                true
            }
            false => false,
        }
    }

    pub(crate) fn register_initialized(&self) -> Result<bool, InvalidStatusUpdate> {
        let updated = self.update_status(|status| match status {
            ActorStatus::Exiting | ActorStatus::Exited(_) => (
                None,
                Err(InvalidStatusUpdate::new(status, ActorStatus::Running)),
            ),
            ActorStatus::Initializing => (Some(ActorStatus::Running), Ok(true)),
            ActorStatus::Running | ActorStatus::Suspended => (None, Ok(false)),
        })?;

        match updated {
            true => {
                tracing::debug!("Process initialized");
                Ok(true)
            }
            false => Ok(false),
        }
    }

    pub(crate) fn register_suspended(&self) -> Result<bool, InvalidStatusUpdate> {
        let updated = self.update_status(|status| match status {
            ActorStatus::Exiting | ActorStatus::Exited(_) | ActorStatus::Initializing => (
                None,
                Err(InvalidStatusUpdate::new(status, ActorStatus::Suspended)),
            ),
            ActorStatus::Suspended => (None, Ok(false)),
            ActorStatus::Running => (Some(ActorStatus::Suspended), Ok(true)),
        })?;

        if updated {
            tracing::debug!("Process suspended");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn register_resumed(&self) -> Result<bool, InvalidStatusUpdate> {
        let updated = self.update_status(|status| match status {
            ActorStatus::Exiting | ActorStatus::Exited(_) | ActorStatus::Initializing => (
                None,
                Err(InvalidStatusUpdate::new(status, ActorStatus::Running)),
            ),
            ActorStatus::Running => (None, Ok(false)),
            ActorStatus::Suspended => (Some(ActorStatus::Running), Ok(true)),
        })?;

        if updated {
            tracing::debug!("Process resumed");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn register_exiting(&self) -> Result<bool, InvalidStatusUpdate> {
        let updated = self.update_status(|status| match status {
            ActorStatus::Exited(_) => (
                None,
                Err(InvalidStatusUpdate::new(status, ActorStatus::Exiting)),
            ),
            ActorStatus::Exiting => (None, Ok(false)),
            ActorStatus::Initializing | ActorStatus::Running | ActorStatus::Suspended => {
                (Some(ActorStatus::Exiting), Ok(true))
            }
        })?;

        match updated {
            true => {
                tracing::debug!("Process exiting");
                Ok(true)
            }
            false => Ok(false),
        }
    }

    pub(crate) fn raw_queue(&self) -> Option<&ConcurrentQueue<C>>
    where
        C: Interface,
    {
        if self.is_interface::<C>() {
            // SAFETY: We just checked that the channel's message queue is of type `ConcurrentQueue<C>`.
            Some(unsafe { self.raw_queue_unchecked() })
        } else {
            None
        }
    }

    /// # Safety
    /// This function is unsafe because it assumes that the channel's message queue is of type
    /// `ConcurrentQueue<T>`. If this assumption is incorrect, it may lead to undefined behavior.
    unsafe fn raw_queue_unchecked(&self) -> &ConcurrentQueue<C>
    where
        C: Interface,
    {
        unsafe {
            &*(&self.data().msg_queue as *const dyn DynamicQueue as *const ConcurrentQueue<C>)
        }
    }

    pub(crate) fn backpressure(&self) -> &BackPressure {
        BackPressure::global()
    }

    pub(crate) async fn delay_for_backpressure(&self) {
        let len = self.data().msg_queue.len();
        let limit = self.data().msg_backpressure_limit;

        if let Some(delay) = self.backpressure().delay(
            self.data().msg_queue.len(),
            self.data().msg_backpressure_limit,
        ) {
            tracing::warn!(
                "Backpressure applied: queue occupancy = {:.2}%, delay = {:?}",
                len as f32 / limit as f32 * 100.0,
                delay
            );
            tokio::time::sleep(delay).await;
        }
    }

    fn _signal(&self, signal: SignalInterface) -> bool {
        if matches!(self.status(), ActorStatus::Exited(_) | ActorStatus::Exiting) {
            return false;
        }

        match self.data().signal_queue.push(signal) {
            Ok(_) => {
                self.data().signal_notifier.notify_one();
            }
            Err(e) => {
                tracing::error!(
                    "Signal queue for {} contains more than {} signals. The signal has been lost. Error: {:?}",
                    std::any::type_name::<Self>(),
                    SIGNAL_QUEUE_CAPACITY,
                    e
                );
                return false;
            }
        }

        true
    }

    pub(crate) fn ref_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
}

impl<I: Interface> Address<I> {
    pub(crate) fn new(pid: Pid, strong_count: usize) -> Self {
        let msg_queue_capacity = match TypeId::of::<I>() == TypeId::of::<Infallible>() {
            true => 1,
            false => MSG_QUEUE_CAPACITY,
        };

        let inner: Arc<Channel<dyn DynamicQueue>> = Arc::new(Channel {
            pid,
            msg_notifier: Notify::new(),
            msg_backpressure_limit: BACKPRESSURE_LIMIT,
            signal_queue: ConcurrentQueue::bounded(SIGNAL_QUEUE_CAPACITY),
            signal_notifier: Notify::new(),
            status_observer: SharedObservable::new(ActorStatus::Exited(ExitStatus::Normal)),
            msg_queue: ConcurrentQueue::<I>::bounded(msg_queue_capacity),
            created_at: Instant::now(),
            spawns: Default::default(),
            exits: Default::default(),
            strong_count: AtomicUsize::new(strong_count),
        });

        Self {
            inner,
            _ctx: PhantomData,
        }
    }

    pub(crate) async fn next_msg(&self) -> Option<I> {
        let raw_queue = self
            .raw_queue()
            .expect("Channel is not of the expected interface type");

        let mut notify = pin!(self.data().msg_notifier.notified());

        loop {
            notify.as_mut().enable();

            if let Ok(msg) = raw_queue.pop() {
                return Some(msg);
            }

            notify.as_mut().await;
            notify.set(self.data().msg_notifier.notified());
        }
    }

    pub(crate) fn pop_msg(&self) -> Option<I> {
        let raw_queue = self
            .raw_queue()
            .expect("Channel is not of the expected interface type");

        raw_queue.pop().ok()
    }

    pub(crate) fn drain_messages_and_signals(&self) {
        while let Some(msg) = self.pop_msg() {
            drop(msg);
        }

        while let Some(signal) = self.pop_signal() {
            let _ = signal;
        }
    }

    pub(crate) async fn next_signal(&self) -> Option<Signal> {
        let mut notify = pin!(self.data().signal_notifier.notified());

        loop {
            notify.as_mut().enable();

            if let Some(signal) = self.pop_signal() {
                return Some(signal);
            }

            notify.as_mut().await;

            notify.set(self.data().signal_notifier.notified());
        }
    }

    pub(crate) fn pop_signal(&self) -> Option<Signal> {
        loop {
            match self.data().signal_queue.pop() {
                Ok(signal) => match self.handle_signal(signal) {
                    Some(event) => return Some(event),
                    None => continue,
                },
                Err(e) => {
                    return match e {
                        PopError::Empty => None,
                        PopError::Closed => unreachable!("Queue should never be closed"),
                    };
                }
            }
        }
    }

    fn handle_signal(&self, signal: SignalInterface) -> Option<Signal> {
        match signal {
            SignalInterface::Shutdown(_) => {
                self.register_exiting().ok();
                Some(Signal::Shutdown)
            }
            SignalInterface::Suspend(_) => {
                if self.status() == ActorStatus::Exiting {
                    tracing::warn!("Actor is exiting, cannot suspend");
                    None
                } else {
                    self.register_suspended().ok();
                    Some(Signal::Suspend)
                }
            }
            SignalInterface::Resume(_) => {
                if self.status() != ActorStatus::Suspended {
                    tracing::warn!("Actor is not suspended, cannot resume");
                    None
                } else {
                    self.register_resumed().ok();
                    Some(Signal::Resume)
                }
            }
            SignalInterface::Ping(envelope) => {
                let _ = envelope.req.reply(());
                None
            }
        }
    }

    pub(crate) async fn next_event(&self, while_exiting: bool) -> Option<InboxEvent<I>> {
        match self.status() {
            ActorStatus::Suspended => self.next_signal().await.map(InboxEvent::Signal),

            ActorStatus::Exited(_) if self.msg_is_empty() => None,

            ActorStatus::Exiting if self.msg_is_empty() && !while_exiting => None,

            _ => {
                select! {
                    biased;

                    Some(signal) = self.next_signal() => Some(InboxEvent::Signal(signal)),
                    Some(msg) = self.next_msg() => Some(InboxEvent::Message(msg)),
                    else => None,
                }
            }
        }
    }

    pub(crate) fn try_next_event(&self) -> Option<InboxEvent<I>> {
        match self.status() {
            ActorStatus::Suspended => self.pop_signal().map(InboxEvent::Signal),
            ActorStatus::Exited(_) if self.msg_is_empty() => None,
            ActorStatus::Exiting if self.msg_is_empty() => None,
            _ => {
                if let Some(signal) = self.pop_signal() {
                    Some(InboxEvent::Signal(signal))
                } else if let Some(msg) = self.pop_msg() {
                    Some(InboxEvent::Message(msg))
                } else {
                    None
                }
            }
        }
    }
}

impl<C: Context> ActorRef for Address<C> {
    type Ctx = C;

    fn actor_ref(&self) -> &Address<Self::Ctx> {
        self
    }
}

impl<C: Context> IntoDyn for Address<C> {
    type Ref<R: Context> = Address<R>;

    fn into_context_unchecked<R>(self) -> Self::Ref<R>
    where
        R: Context,
    {
        Address {
            inner: self.inner,
            _ctx: PhantomData,
        }
    }
}

impl<T: Context> Clone for Address<T> {
    fn clone(&self) -> Self {
        self._clone()
    }
}

impl<C: Context> Debug for Address<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Address")
            .field("pid", &self.data().pid)
            .field("status", &self.data().status_observer.get())
            .field("len", &self.data().msg_queue.len())
            .finish()
    }
}

impl<C: Context> Eq for Address<C> {}
impl<C: Context> PartialEq for Address<C> {
    fn eq(&self, other: &Self) -> bool {
        self.data().pid == other.data().pid
    }
}
impl<C: Context> Hash for Address<C> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.data().pid.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Interface)]
    #[interface(path = "zestors_interface")]
    pub enum MyInterface {
        A(Envelope<u32>),
        AB(Envelope<u64>),
    }

    #[tokio::test]
    async fn test_address_downcast_ref() {
        let child = crate::spawn(|_: Inbox<MyInterface>| async move { Ok(()) });
        let address = child.address().clone().into_dyn::<()>();

        address
            .downcast::<MyInterface>()
            .expect("Should downcast to MyInterface");
    }
}
