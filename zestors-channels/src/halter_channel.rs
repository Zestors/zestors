use event_listener::{Event, EventListener};
use std::{
    any::Any,
    sync::{
        atomic::{AtomicI32, AtomicUsize, Ordering},
        Arc,
    },
};
use zestors_core::{
    actor_kind::ActorKind,
    channel::{ActorId, Channel, DynChannel},
    config::Capacity,
    messaging::{AnyMessage, SendUncheckedError, TrySendUncheckedError},
    *,
};

use crate::halter::Halter;

/// A [Channel] that does not have any kind of inbox, so it cannot receive messages.
#[derive(Debug)]
pub struct HalterChannel {
    address_count: AtomicUsize,
    halter_count: AtomicUsize,
    to_halt: AtomicI32,
    actor_id: ActorId,
    exit_event: Event,
    halt_event: Event,
}

impl ActorKind for Halter {
    type Channel = HalterChannel;

    fn into_dyn_channel(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel> {
        channel
    }
}

impl HalterChannel {
    pub(crate) fn new(address_count: usize, halter_count: usize, actor_id: ActorId) -> Self {
        Self {
            address_count: AtomicUsize::new(address_count),
            halter_count: AtomicUsize::new(halter_count),
            to_halt: AtomicI32::new(0),
            actor_id,
            exit_event: Event::new(),
            halt_event: Event::new(),
        }
    }

    pub(crate) fn get_halt_listener(&self) -> EventListener {
        self.halt_event.listen()
    }

    pub(crate) fn should_halt(&self) -> bool {
        let halt_count = self.to_halt.fetch_sub(1, Ordering::AcqRel);
        if halt_count > 0 {
            true
        } else {
            if halt_count < -1000 {
                // If it was smaller than -1000, reset the halt-count to 0 to prevent overflow.
                self.to_halt
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |x| {
                        // Check here again, since it might have been updated in the mean time.
                        if x < -1_000 {
                            Some(0)
                        } else {
                            Some(x)
                        }
                    })
                    .unwrap();
            };
            false
        }
    }

    pub(crate) fn decrement_halter_count(&self, n: usize) {
        self.halter_count.fetch_sub(n, Ordering::AcqRel);
    }
}

impl Channel for HalterChannel {
    fn close(&self) -> bool {
        false
    }
    fn halt_some(&self, n: u32) {
        let n = n.try_into().unwrap_or(i32::MAX);
        self.to_halt
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |x| {
                Some(x.saturating_add(n))
            })
            .unwrap();
        self.halt_event.notify(n as usize)
    }
    fn halt(&self) {
        self.to_halt.store(i32::MAX, Ordering::Release);
        self.halt_event.notify(usize::MAX)
    }
    fn process_count(&self) -> usize {
        self.halter_count.load(Ordering::Acquire)
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
        self.halter_count.load(Ordering::Acquire) == 0
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
    fn try_add_inbox(&self) -> Result<usize, ()> {
        let result = self
            .halter_count
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

impl DynChannel for HalterChannel {
    fn try_send_boxed(&self, boxed: AnyMessage) -> Result<(), TrySendUncheckedError<AnyMessage>> {
        Err(TrySendUncheckedError::NotAccepted(boxed))
    }

    fn send_now_boxed(&self, boxed: AnyMessage) -> Result<(), TrySendUncheckedError<AnyMessage>> {
        Err(TrySendUncheckedError::NotAccepted(boxed))
    }

    fn send_blocking_boxed(&self, boxed: AnyMessage) -> Result<(), SendUncheckedError<AnyMessage>> {
        Err(SendUncheckedError::NotAccepted(boxed))
    }

    fn send_boxed<'a>(
        &'a self,
        boxed: AnyMessage,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<(), SendUncheckedError<AnyMessage>>> + Send + 'a>,
    > {
        Box::pin(async move { Err(SendUncheckedError::NotAccepted(boxed)) })
    }

    fn accepts(&self, _id: &std::any::TypeId) -> bool {
        false
    }

    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}
