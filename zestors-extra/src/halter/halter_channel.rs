use event_listener::{Event, EventListener};
use std::{
    any::Any,
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
        Arc,
    },
};
use zestors_core::{
    inboxes::Capacity,
    messaging::{AnyPayload, SendCheckedError, TrySendCheckedError},
    monitoring::{ActorId, Channel, TryAddProcessError},
    *,
};

/// A [Channel] that does not have any kind of inbox, so it cannot receive messages.
#[derive(Debug)]
pub struct HalterChannel {
    address_count: AtomicUsize,
    to_halt: AtomicBool,
    has_exited: AtomicBool,
    actor_id: ActorId,
    exit_event: Event,
    halt_event: Event,
}

impl HalterChannel {
    pub(crate) fn new(address_count: usize, actor_id: ActorId) -> Self {
        Self {
            address_count: AtomicUsize::new(address_count),
            to_halt: AtomicBool::new(false),
            has_exited: AtomicBool::new(false),
            actor_id,
            exit_event: Event::new(),
            halt_event: Event::new(),
        }
    }

    pub(crate) fn get_halt_listener(&self) -> EventListener {
        self.halt_event.listen()
    }

    pub(crate) fn should_halt(&self) -> bool {
        self.to_halt.load(Ordering::Acquire)
    }

    pub(crate) fn exit(&self) {
        self.has_exited.store(true, Ordering::Release)
    }
}

impl Channel for HalterChannel {
    fn close(&self) -> bool {
        false
    }

    fn halt_some(&self, n: u32) {
        self.halt()
    }

    fn halt(&self) {
        self.to_halt.store(true, Ordering::Release);
        self.halt_event.notify(1)
    }

    fn process_count(&self) -> usize {
        if self.has_exited() {
            0
        } else {
            1
        }
    }

    fn msg_count(&self) -> usize {
        0
    }

    fn address_count(&self) -> usize {
        self.address_count.load(Ordering::Acquire)
    }

    fn is_closed(&self) -> bool {
        true
    }

    fn has_exited(&self) -> bool {
        self.has_exited.load(Ordering::Acquire)
    }

    fn get_exit_listener(&self) -> EventListener {
        self.exit_event.listen()
    }

    fn actor_id(&self) -> ActorId {
        self.actor_id
    }

    fn capacity(&self) -> &Capacity {
        static CAPACITY: Capacity = Capacity::Bounded(0);
        &CAPACITY
    }

    fn add_address(&self) -> usize {
        self.address_count.fetch_add(1, Ordering::Acquire)
    }

    fn remove_address(&self) -> usize {
        let prev_address_count = self.address_count.fetch_sub(1, Ordering::Acquire);
        assert!(prev_address_count >= 1);
        prev_address_count
    }

    fn try_add_process(&self) -> Result<usize, TryAddProcessError> {
        Err(TryAddProcessError::SingleInboxOnly)
    }

    fn try_send_any(&self, boxed: AnyPayload) -> Result<(), TrySendCheckedError<AnyPayload>> {
        Err(TrySendCheckedError::NotAccepted(boxed))
    }

    fn force_send_any(&self, boxed: AnyPayload) -> Result<(), TrySendCheckedError<AnyPayload>> {
        Err(TrySendCheckedError::NotAccepted(boxed))
    }

    fn send_any_blocking(&self, boxed: AnyPayload) -> Result<(), SendCheckedError<AnyPayload>> {
        Err(SendCheckedError::NotAccepted(boxed))
    }

    fn send_any<'a>(
        &'a self,
        boxed: AnyPayload,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<(), SendCheckedError<AnyPayload>>> + Send + 'a>,
    > {
        Box::pin(async move { Err(SendCheckedError::NotAccepted(boxed)) })
    }

    fn accepts(&self, _id: &std::any::TypeId) -> bool {
        false
    }

    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn into_dyn(self: Arc<Self>) -> Arc<dyn Channel> {
        self
    }
}
