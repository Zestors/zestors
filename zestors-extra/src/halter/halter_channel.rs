use crate::halter::Halter;
use event_listener::{Event, EventListener};
use std::{
    any::Any,
    sync::{
        atomic::{AtomicI32, AtomicUsize, Ordering},
        Arc,
    },
};
use zestors_core::{
    inboxes::Capacity,
    messaging::{AnyMessage, SendCheckedError, TrySendCheckedError},
    monitoring::{ActorId, Channel},
    *,
};

/// A [Channel] that does not have any kind of inbox, so it cannot receive messages.
#[derive(Debug)]
pub struct CoreChannel {
    /// The id of the actor
    actor_id: ActorId,
    /// The amount of addresses
    address_count: AtomicUsize,
    /// The amount of processes
    process_count: AtomicUsize,
    /// The halt-count
    to_halt_count: AtomicI32,
    /// Subscribe to be notified when the actor exits.
    exit_event: Event,
    /// Subscribe to be notified when a process is halted.
    halt_event: Event,
}

impl CoreChannel {
    pub(crate) fn new(address_count: usize, halter_count: usize, actor_id: ActorId) -> Self {
        Self {
            address_count: AtomicUsize::new(address_count),
            process_count: AtomicUsize::new(halter_count),
            to_halt_count: AtomicI32::new(0),
            actor_id,
            exit_event: Event::new(),
            halt_event: Event::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn halt_count(&self) -> i32 {
        self.to_halt_count.load(Ordering::Acquire)
    }

    pub(crate) fn get_halt_listener(&self) -> EventListener {
        self.halt_event.listen()
    }

    /// Whether the
    pub fn should_halt(&self) -> bool {
        // If the count is bigger than 0, we might have to halt.
        if self.to_halt_count.load(Ordering::Acquire) > 0 {
            // If after before subtraction the count was 1 or larger, we should halt.
            if self.to_halt_count.fetch_sub(1, Ordering::AcqRel) >= 1 {
                return true;
            }
        };
        false
    }

    /// Removes 1 from process_count and returns:
    /// - `true` if the `process_count` is now `0`. This also notifies all exit_listeners.
    /// - `false` otherwise.
    ///
    /// # Panics
    /// If the old count was 0.
    pub fn remove_process(&self) -> bool {
        let prev_count = self.process_count.fetch_sub(1, Ordering::AcqRel);
        match prev_count {
            0 => panic!(),
            1 => {
                self.exit_event.notify(usize::MAX);
                true
            }
            _ => false,
        }
    }

    fn halt_some(&self, n: u32) {
        let n = n.try_into().unwrap_or(i32::MAX);
        self.to_halt_count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                if count <= 0 {
                    Some(n)
                } else {
                    Some(count.saturating_add(n))
                }
            })
            .unwrap();
        self.to_halt_count.load(Ordering::Acquire);
        self.halt_event.notify(usize::MAX)
    }

    fn halt(&self) {
        self.to_halt_count.store(i32::MAX, Ordering::Release);
        self.halt_event.notify(usize::MAX)
    }

    fn process_count(&self) -> usize {
        self.process_count.load(Ordering::Acquire)
    }

    fn address_count(&self) -> usize {
        self.address_count.load(Ordering::Acquire)
    }

    fn has_exited(&self) -> bool {
        self.process_count.load(Ordering::Acquire) == 0
    }

    fn get_exit_listener(&self) -> EventListener {
        self.exit_event.listen()
    }

    fn actor_id(&self) -> ActorId {
        self.actor_id
    }

    fn add_address(&self) -> usize {
        self.address_count.fetch_add(1, Ordering::Acquire)
    }

    fn remove_address(&self) -> usize {
        let prev_address_count = self.address_count.fetch_sub(1, Ordering::Acquire);
        assert!(prev_address_count >= 1);
        prev_address_count
    }

    fn try_add_inbox(&self) -> Result<usize, ()> {
        let result = self
            .process_count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |val| {
                if val < 1 {
                    None
                } else {
                    Some(val + 1)
                }
            });

        match result {
            Ok(prev) => Ok(prev),
            Err(_) => Err(()),
        }
    }
}

impl Channel for CoreChannel {
    // Core
    fn halt_some(&self, n: u32) {
        self.halt_some(n)
    }
    fn halt(&self) {
        self.halt()
    }
    fn process_count(&self) -> usize {
        self.process_count()
    }
    fn address_count(&self) -> usize {
        self.address_count()
    }
    fn has_exited(&self) -> bool {
        self.has_exited()
    }
    fn get_exit_listener(&self) -> EventListener {
        self.get_exit_listener()
    }
    fn actor_id(&self) -> ActorId {
        self.actor_id()
    }
    fn add_address(&self) -> usize {
        self.add_address()
    }
    fn remove_address(&self) -> usize {
        self.remove_address()
    }
    fn try_add_inbox(&self) -> Result<usize, ()> {
        self.try_add_inbox()
    }

    // Not applicable
    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
    fn into_dyn(self: Arc<Self>) -> Arc<dyn Channel> {
        self
    }

    // To remove
    fn close(&self) -> bool {
        false
    }
    fn msg_count(&self) -> usize {
        0
    }
    fn is_closed(&self) -> bool {
        true
    }
    fn try_send_boxed(&self, boxed: AnyMessage) -> Result<(), TrySendCheckedError<AnyMessage>> {
        Err(TrySendCheckedError::NotAccepted(boxed))
    }
    fn capacity(&self) -> &Capacity {
        static CAPACITY: Capacity = Capacity::Bounded(0);
        &CAPACITY
    }
    fn send_now_boxed(&self, boxed: AnyMessage) -> Result<(), TrySendCheckedError<AnyMessage>> {
        Err(TrySendCheckedError::NotAccepted(boxed))
    }

    fn send_blocking_boxed(&self, boxed: AnyMessage) -> Result<(), SendCheckedError<AnyMessage>> {
        Err(SendCheckedError::NotAccepted(boxed))
    }
    fn send_boxed<'a>(
        &'a self,
        boxed: AnyMessage,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<(), SendCheckedError<AnyMessage>>> + Send + 'a>,
    > {
        Box::pin(async move { Err(SendCheckedError::NotAccepted(boxed)) })
    }
    fn accepts(&self, _id: &std::any::TypeId) -> bool {
        false
    }
}
