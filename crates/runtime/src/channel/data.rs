use super::*;

pub(crate) struct Channel<Q: ?Sized> {
    pub(crate) pid: Pid,
    pub(crate) signal_queue: ConcurrentQueue<SignalInterface>,
    pub(crate) signal_notifier: Notify,
    pub(crate) status_observer: SharedObservable<ActorStatus>,
    pub(crate) msg_notifier: Notify,
    pub(crate) msg_backpressure_limit: usize,
    pub(crate) created_at: Instant,
    pub(crate) spawns: RwLock<Vec<Instant>>,
    pub(crate) exits: RwLock<Vec<(Instant, Result<(), ExitError>)>>,
    pub(crate) strong_count: AtomicUsize,
    pub(crate) msg_queue: Q,
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
