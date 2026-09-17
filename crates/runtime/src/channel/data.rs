use super::*;
use eyeball::ObservableWriteGuard;

pub(crate) struct Channel<Q: ?Sized> {
    pid: Pid,
    signal_queue: ConcurrentQueue<SignalInterface>,
    signal_notifier: Notify,
    status_observer: SharedObservable<ActorStatus>,
    msg_notifier: Notify,
    msg_backpressure_limit: usize,
    created_at: Instant,
    spawns: RwLock<Vec<Instant>>,
    exits: RwLock<Vec<(Instant, Result<(), ExitError>)>>,
    strong_count: AtomicUsize,
    msg_queue: Q,
}

impl<Q> Channel<Q> {
    pub(crate) fn new(pid: Pid, strong_count: usize, msg_queue: Q) -> Self {
        Self {
            pid,
            msg_notifier: Notify::new(),
            msg_backpressure_limit: BACKPRESSURE_LIMIT,
            signal_queue: ConcurrentQueue::bounded(SIGNAL_QUEUE_CAPACITY),
            signal_notifier: Notify::new(),
            status_observer: SharedObservable::new(ActorStatus::Exited(ExitStatus::Normal)),
            msg_queue,
            created_at: Instant::now(),
            spawns: Default::default(),
            exits: Default::default(),
            strong_count: AtomicUsize::new(strong_count),
        }
    }
}

impl Channel<dyn DynamicQueue> {
    pub(crate) fn msg_len(&self) -> usize {
        self.msg_queue.len()
    }

    pub(crate) fn signal_len(&self) -> usize {
        self.signal_queue.len()
    }

    pub(crate) fn backpressure_limit(&self) -> usize {
        self.msg_backpressure_limit
    }

    pub(crate) fn pid(&self) -> &Pid {
        &self.pid
    }

    pub(crate) fn status(&self) -> ActorStatus {
        self.status_observer.get()
    }

    pub(crate) fn members(&self) -> &'static [TypeId] {
        self.msg_queue.members()
    }

    pub(crate) fn signal(&self, signal: SignalInterface) -> bool {
        if matches!(self.status(), ActorStatus::Exited(_) | ActorStatus::Exiting) {
            return false;
        }

        match self.signal_queue.push(signal) {
            Ok(_) => {
                self.signal_notifier.notify_one();
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

    pub(crate) fn created_at(&self) -> Instant {
        self.created_at
    }

    pub(crate) fn last_spawned_at(&self) -> Option<Instant> {
        let spawned_at = self.spawns.read().unwrap();
        spawned_at.last().cloned()
    }

    pub(crate) fn spawned_at(&self) -> Vec<Instant> {
        let spawned_at = self.spawns.read().unwrap();
        spawned_at.clone()
    }

    pub(crate) fn strong_count(&self) -> usize {
        self.strong_count.load(Ordering::Relaxed)
    }

    /// Increments the strong reference count. The caller must already own a
    /// strong reference.
    pub(crate) fn incr_strong_count(&self) {
        // Relaxed is sufficient because the caller already owns a strong reference
        let prev_count = self.strong_count.fetch_add(1, Ordering::Relaxed);

        // Prevent integer overflow attack/bug
        if prev_count > usize::MAX / 2 {
            std::process::abort();
        }
    }

    /// Decrements the strong reference count, returning `true` if this was
    /// the last strong reference (i.e. the count just reached zero).
    pub(crate) fn decr_strong_count(&self) -> bool {
        let prev_count = self.strong_count.fetch_sub(1, Ordering::Release);

        // fetch_sub returns the PREVIOUS value. If it was 1, it is now 0.
        if prev_count == 1 {
            // Synchronize memory access from other threads before cleaning up
            std::sync::atomic::fence(Ordering::Acquire);
            true
        } else {
            false
        }
    }

    pub(crate) async fn watch<T>(
        &self,
        mut check_for: impl FnMut(ActorStatus) -> Option<T> + Send + 'static,
    ) -> T {
        let mut subscriber = self.status_observer.subscribe();

        loop {
            let status = self.status();

            if let Some(result) = check_for(status) {
                return result;
            }

            let status = subscriber.next().await;

            if let Some(status) = status {
                if let Some(result) = check_for(status) {
                    return result;
                }
            }
        }
    }

    pub(crate) fn is_interface<I: Interface>(&self) -> bool {
        self.msg_queue.type_id() == TypeId::of::<ConcurrentQueue<I>>()
    }

    pub(crate) fn exits(&self) -> Vec<(Instant, Result<(), ExitError>)> {
        let exits = self.exits.read().unwrap();
        exits.clone()
    }

    pub(crate) fn try_push_msg<M: Message>(&self, msg: M) -> Result<M::Receipt, NotAccepted<M>> {
        self.msg_queue.try_push_msg(msg)
    }

    pub(crate) fn msg_notify_one(&self) {
        self.msg_notifier.notify_one();
    }

    #[expect(unused)]
    pub(crate) fn pop_dyn(&self) -> Result<AnyEnvelope, PopError> {
        self.msg_queue.pop_dyn()
    }

    fn update_status<F, T>(&self, f: F) -> T
    where
        F: FnOnce(ActorStatus) -> (Option<ActorStatus>, T),
    {
        let mut observer = self.status_observer.write();
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

        let mut spawned_at = self.spawns.write().unwrap();

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

                let mut exited_at = self.exits.write().unwrap();

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

    pub(crate) fn raw_queue<C: Interface>(&self) -> Option<&ConcurrentQueue<C>> {
        if self.is_interface::<C>() {
            // SAFETY: We just checked that the channel's message queue is of type `ConcurrentQueue<C>`.
            Some(unsafe { self.raw_queue_unchecked::<C>() })
        } else {
            None
        }
    }

    /// # Safety
    /// This function is unsafe because it assumes that the channel's message queue is of type
    /// `ConcurrentQueue<T>`. If this assumption is incorrect, it may lead to undefined behavior.
    unsafe fn raw_queue_unchecked<C: Interface>(&self) -> &ConcurrentQueue<C> {
        unsafe { &*(&self.msg_queue as *const dyn DynamicQueue as *const ConcurrentQueue<C>) }
    }

    pub(crate) async fn delay_for_backpressure(&self) {
        let len = self.msg_queue.len();
        let limit = self.msg_backpressure_limit;

        if let Some(delay) = BackPressure::global().delay(len, limit) {
            tracing::warn!(
                "Backpressure applied: queue occupancy = {:.2}%, delay = {:?}",
                len as f32 / limit as f32 * 100.0,
                delay
            );
            tokio::time::sleep(delay).await;
        }
    }

    pub(crate) async fn next_msg<I: Interface>(&self) -> Option<I> {
        let raw_queue = self
            .raw_queue::<I>()
            .expect("Channel is not of the expected interface type");

        let mut notify = pin!(self.msg_notifier.notified());

        loop {
            notify.as_mut().enable();

            if let Ok(msg) = raw_queue.pop() {
                return Some(msg);
            }

            notify.as_mut().await;
            notify.set(self.msg_notifier.notified());
        }
    }

    pub(crate) fn pop_msg<I: Interface>(&self) -> Option<I> {
        let raw_queue = self
            .raw_queue::<I>()
            .expect("Channel is not of the expected interface type");

        raw_queue.pop().ok()
    }

    pub(crate) async fn next_signal(&self) -> Option<Signal> {
        let mut notify = pin!(self.signal_notifier.notified());

        loop {
            notify.as_mut().enable();

            if let Some(signal) = self.pop_signal() {
                return Some(signal);
            }

            notify.as_mut().await;

            notify.set(self.signal_notifier.notified());
        }
    }

    pub(crate) fn pop_signal(&self) -> Option<Signal> {
        loop {
            match self.signal_queue.pop() {
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InvalidStatusUpdate {
    pub from: ActorStatus,
    pub to: ActorStatus,
}

impl InvalidStatusUpdate {
    pub fn new(from: ActorStatus, to: ActorStatus) -> Self {
        Self { from, to }
    }
}
