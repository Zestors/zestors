use event_listener::{Event, EventListener};
use std::{
    any::Any,
    sync::{
        atomic::{AtomicI32, AtomicUsize, Ordering},
        Arc,
    },
};
use zestors_core::{
    actor_type::ActorType,
    inboxes::Capacity,
    messaging::{AnyMessage, SendCheckedError, TrySendCheckedError},
    monitoring::{ActorId, Channel},
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

impl ActorType for Halter {
    type Channel = HalterChannel;
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

    #[cfg(test)]
    pub(crate) fn to_halt(&self) -> i32 {
        self.to_halt.load(Ordering::Acquire)
    }

    pub(crate) fn get_halt_listener(&self) -> EventListener {
        self.halt_event.listen()
    }

    pub(crate) fn should_halt(&self) -> bool {
        //     // If the count is bigger than 0, we might have to halt.
        //     if self.halt_count.load(Ordering::Acquire) > 0 {
        //         // Now subtract 1 from the count
        //         let prev_count = self.halt_count.fetch_sub(1, Ordering::AcqRel);
        //         // If the count before updating was bigger than 0, we halt.
        //         // If this decrements below 0, we treat it as if it's 0.
        //         if prev_count > 0 {
        //             return true;
        //         }
        //     }

        //     // Otherwise, just continue
        //     false
        let halt_count = dbg!(self.to_halt.fetch_sub(1, Ordering::AcqRel));
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

    pub(crate) fn decrement_halter_count(&self, n: usize) -> usize {
        self.halter_count.fetch_sub(n, Ordering::AcqRel)
    }

    /// Removes a halter and returns the previous halter_count.
    ///
    /// - Panics if the old count was 0.
    /// - Triggers an exit if the old count was 1.
    pub(crate) fn remove_halter(&self) -> usize {
        let prev_count = self.decrement_halter_count(1);
        match prev_count {
            0 => panic!(),
            1 => self.exit_event.notify(usize::MAX),
            _ => (),
        };
        prev_count
    }
}

impl Channel for HalterChannel {
    fn close(&self) -> bool {
        false
    }
    fn halt_some(&self, n: u32) {
        let n = dbg!(n.try_into().unwrap_or(i32::MAX));
        let res = self
            .to_halt
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |x| {
                if x <= 0 {
                    Some(n)
                } else {
                    Some(x.saturating_add(n))
                }
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

    fn try_send_boxed(&self, boxed: AnyMessage) -> Result<(), TrySendCheckedError<AnyMessage>> {
        Err(TrySendCheckedError::NotAccepted(boxed))
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

    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn into_dyn(self: Arc<Self>) -> Arc<dyn Channel> {
        self
    }
}
