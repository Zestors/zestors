use super::*;
use concurrent_queue::{ConcurrentQueue, PopError, PushError};
use event_listener::{Event, EventListener};
use futures::future::BoxFuture;
use std::{
    any::{Any, TypeId},
    sync::{atomic::Ordering, Arc},
};
use std::{
    fmt::Debug,
    sync::atomic::{AtomicI32, AtomicUsize},
};

pub(crate) use receiving::RecvRawFut;
mod receiving;
mod sending;

/// A [Channel] with an inbox used to receive messages.
pub struct InboxChannel<M> {
    /// The underlying queue
    queue: ConcurrentQueue<M>,
    /// The capacity of the channel
    capacity: Capacity,
    /// The amount of addresses associated to this channel.
    /// Once this is 0, not more addresses can be created and the Channel is closed.
    address_count: AtomicUsize,
    /// The amount of inboxes associated to this channel.
    /// Once this is 0, it is impossible to spawn new processes, and the Channel.
    /// has exited.
    inbox_count: AtomicUsize,
    /// Subscribe when trying to receive a message from this channel.
    recv_event: Event,
    /// Subscribe when trying to send a message into this channel.
    send_event: Event,
    /// Subscribe when waiting for Actor to exit.
    exit_event: Event,
    /// The amount of processes that should still be halted.
    /// Can be negative bigger than amount of processes in total.
    halt_count: AtomicI32,
    /// The actor_id, generated once and cannot be changed afterwards.
    actor_id: ActorId,
}

impl<M> InboxChannel<M> {
    /// Create a new channel, given an address count, inbox_count and capacity.
    ///
    /// After this, it must be ensured that the correct amount of inboxes and addresses actually exist.
    pub(crate) fn new(address_count: usize, inbox_count: usize, capacity: Capacity) -> Self {
        Self {
            queue: match &capacity {
                Capacity::Bounded(size) => ConcurrentQueue::bounded(size.to_owned()),
                Capacity::Unbounded(_) => ConcurrentQueue::unbounded(),
            },
            capacity,
            address_count: AtomicUsize::new(address_count),
            inbox_count: AtomicUsize::new(inbox_count),
            recv_event: Event::new(),
            send_event: Event::new(),
            exit_event: Event::new(),
            halt_count: AtomicI32::new(0),
            actor_id: ActorId::generate_new(),
        }
    }

    /// Remove an Inbox from the channel, decrementing inbox-count by 1. This should be
    /// called during the Inbox's destructor.
    ///
    /// If there are no more Inboxes remaining, this will close the channel and set
    /// `inboxes_dropped` to true. This will also drop any messages still inside the
    /// channel.
    ///
    /// Returns the previous inbox-count.
    ///
    /// ## Notifies
    /// * `prev-inbox-count == 1` -> all exit-listeners
    ///
    /// ## Panics
    /// * `prev-inbox-count == 0`
    pub(crate) fn remove_inbox(&self) -> usize {
        // Subtract one from the inbox count
        let prev_count = self.inbox_count.fetch_sub(1, Ordering::AcqRel);
        assert!(prev_count != 0);

        // If previous count was 1, then all inboxes have been dropped.
        if prev_count == 1 {
            self.close();
            // Also notify the exit-listeners, since the process exited.
            self.exit_event.notify(usize::MAX);
            // drop all messages, since no more inboxes exist.
            while self.pop_msg().is_ok() {}
        }

        prev_count
    }

    /// Takes the next message out of the channel.
    ///
    /// Returns an error if the queue is closed, returns none if there is no message
    /// in the queue.
    ///
    /// ## Notifies
    /// on success -> 1 send_listener & 1 recv_listener
    pub(crate) fn pop_msg(&self) -> Result<M, PopError> {
        self.queue.pop().map(|msg| {
            self.send_event.notify(1);
            self.recv_event.notify(1);
            msg
        })
    }

    /// Push a message into the channel.
    ///
    /// Can fail either because the queue is full, or because it is closed.
    ///
    /// ## Notifies
    /// on success -> 1 recv_listener
    pub(crate) fn push_msg(&self, msg: M) -> Result<(), PushError<M>> {
        match self.queue.push(msg) {
            Ok(()) => {
                self.recv_event.notify(1);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Can be called by an inbox to know whether it should halt.
    ///
    /// This decrements the halt-counter by one when it is called, therefore every
    /// inbox should only receive true from this method once! The inbox keeps it's own
    /// local state about whether it has received true from this method.
    pub(crate) fn inbox_should_halt(&self) -> bool {
        // If the count is bigger than 0, we might have to halt.
        if self.halt_count.load(Ordering::Acquire) > 0 {
            // Now subtract 1 from the count
            let prev_count = self.halt_count.fetch_sub(1, Ordering::AcqRel);
            // If the count before updating was bigger than 0, we halt.
            // If this decrements below 0, we treat it as if it's 0.
            if prev_count > 0 {
                return true;
            }
        }

        // Otherwise, just continue
        false
    }

    /// Get a new recv-event listener
    pub(crate) fn get_recv_listener(&self) -> EventListener {
        self.recv_event.listen()
    }

    /// Get a new send-event listener
    pub(crate) fn get_send_listener(&self) -> EventListener {
        self.send_event.listen()
    }

    /// Get a new exit-event listener
    pub(crate) fn get_exit_listener(&self) -> EventListener {
        self.exit_event.listen()
    }
}

impl<T> Channel for InboxChannel<T> {
    /// Close the channel. Returns `true` if the channel was not closed before this.
    /// Otherwise, this returns `false`.
    ///
    /// ## Notifies
    /// * if `true` -> all send_listeners & recv_listeners
    fn close(&self) -> bool {
        if self.queue.close() {
            self.recv_event.notify(usize::MAX);
            self.send_event.notify(usize::MAX);
            true
        } else {
            false
        }
    }

    /// Halt n inboxes associated with this channel. If `n >= #inboxes`, all inboxes
    /// will be halted. This might leave `halt-count > inbox-count`, however that's not
    /// a problem. If n > i32::MAX, n = i32::MAX.
    ///
    /// # Notifies
    /// * all recv-listeners
    fn halt_some(&self, n: u32) {
        let n = i32::try_from(n).unwrap_or(i32::MAX);

        self.halt_count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                // If the count < 0, act as if it's 0.
                if count < 0 {
                    Some(n)
                } else {
                    // Otherwise, add both together.
                    Some(count.saturating_add(n))
                }
            })
            .unwrap();

        self.recv_event.notify(usize::MAX);
    }

    /// Returns the amount of inboxes this channel has.
    fn process_count(&self) -> usize {
        self.inbox_count.load(Ordering::Acquire)
    }

    /// Returns the amount of messages currently in the channel.
    fn msg_count(&self) -> usize {
        self.queue.len()
    }

    /// Returns the amount of addresses this channel has.
    fn address_count(&self) -> usize {
        self.address_count.load(Ordering::Acquire)
    }

    /// Whether the queue asscociated to the channel has been closed.
    fn is_closed(&self) -> bool {
        self.queue.is_closed()
    }

    /// Capacity of the inbox.
    fn capacity(&self) -> &Capacity {
        &self.capacity
    }

    /// Whether all inboxes linked to this channel have exited.
    fn has_exited(&self) -> bool {
        self.inbox_count.load(Ordering::Acquire) == 0
    }

    /// Add an Address to the channel, incrementing address-count by 1. Afterwards,
    /// a new Address may be created from this channel.
    ///
    /// Returns the previous inbox-count
    fn add_address(&self) -> usize {
        self.address_count.fetch_add(1, Ordering::AcqRel)
    }

    /// Remove an Address from the channel, decrementing address-count by 1. This should
    /// be called from the destructor of the Address.
    ///
    /// ## Notifies
    /// * `prev-address-count == 1` -> all send_listeners & recv_listeners
    ///
    /// ## Panics
    /// * `prev-address-count == 0`
    fn remove_address(&self) -> usize {
        // Subtract one from the inbox count
        let prev_address_count = self.address_count.fetch_sub(1, Ordering::AcqRel);
        assert!(prev_address_count >= 1);
        prev_address_count
    }

    fn get_exit_listener(&self) -> EventListener {
        self.get_exit_listener()
    }

    /// Get the actor_id.
    fn actor_id(&self) -> ActorId {
        self.actor_id
    }

    // Closes the channel and halts all actors.
    fn halt(&self) {
        self.close();
        self.halt_some(u32::MAX);
    }

    fn try_add_inbox(&self) -> Result<usize, ()> {
        let result = self
            .inbox_count
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

impl<P: Protocol + Send> DynChannel for InboxChannel<P> {
    fn try_send_boxed(
        &self,
        boxed: BoxedMessage,
    ) -> Result<(), TrySendUncheckedError<BoxedMessage>> {
        match P::try_from_box(boxed) {
            Ok(prot) => self.try_send_raw(prot).map_err(|e| match e {
                TrySendError::Full(prot) => TrySendUncheckedError::Full(prot.into_box()),
                TrySendError::Closed(prot) => TrySendUncheckedError::Closed(prot.into_box()),
            }),
            Err(boxed) => Err(TrySendUncheckedError::NotAccepted(boxed)),
        }
    }

    fn send_now_boxed(
        &self,
        boxed: BoxedMessage,
    ) -> Result<(), TrySendUncheckedError<BoxedMessage>> {
        match P::try_from_box(boxed) {
            Ok(prot) => self.send_raw_now(prot).map_err(|e| match e {
                TrySendError::Full(prot) => TrySendUncheckedError::Full(prot.into_box()),
                TrySendError::Closed(prot) => TrySendUncheckedError::Closed(prot.into_box()),
            }),
            Err(boxed) => Err(TrySendUncheckedError::NotAccepted(boxed)),
        }
    }

    fn send_blocking_boxed(
        &self,
        boxed: BoxedMessage,
    ) -> Result<(), SendUncheckedError<BoxedMessage>> {
        match P::try_from_box(boxed) {
            Ok(prot) => self
                .send_raw_blocking(prot)
                .map_err(|SendError(prot)| SendUncheckedError::Closed(prot.into_box())),
            Err(boxed) => Err(SendUncheckedError::NotAccepted(boxed)),
        }
    }

    fn send_boxed(
        &self,
        boxed: BoxedMessage,
    ) -> BoxFuture<'_, Result<(), SendUncheckedError<BoxedMessage>>> {
        Box::pin(async move {
            match P::try_from_box(boxed) {
                Ok(prot) => self
                    .send_raw(prot)
                    .await
                    .map_err(|SendError(prot)| SendUncheckedError::Closed(prot.into_box())),
                Err(boxed) => Err(SendUncheckedError::NotAccepted(boxed)),
            }
        })
    }

    fn accepts(&self, id: &TypeId) -> bool {
        <P as Protocol>::accepts_msg(id)
    }

    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

impl<M> Debug for InboxChannel<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel")
            .field("queue", &self.queue)
            .field("capacity", &self.capacity)
            .field("address_count", &self.address_count)
            .field("inbox_count", &self.inbox_count)
            .field("halt_count", &self.halt_count)
            .finish()
    }
}

#[cfg(test)]
mod test {
    use std::{
        sync::{atomic::Ordering, Arc},
        time::Duration,
    };

    use super::InboxChannel;
    use crate::*;
    use concurrent_queue::{PopError, PushError};
    use event_listener::EventListener;
    use futures::FutureExt;

    #[test]
    fn channels_have_actor_ids() {
        let id1 = InboxChannel::<()>::new(1, 1, Capacity::Bounded(10)).actor_id();
        let id2 = InboxChannel::<()>::new(1, 1, Capacity::Bounded(10)).actor_id();
        assert!(id1 < id2);
    }

    #[test]
    fn capacity_types_are_correct() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::Bounded(10));
        assert!(channel.queue.capacity().is_some());
        assert!(channel.capacity().is_bounded());
        let channel = InboxChannel::<()>::new(1, 1, Capacity::Unbounded(BackPressure::default()));
        assert!(channel.queue.capacity().is_none());
        assert!(!channel.capacity().is_bounded());
    }

    #[test]
    fn adding_removing_addresses() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default());
        assert_eq!(channel.address_count(), 1);
        channel.add_address();
        assert_eq!(channel.address_count(), 2);
        channel.remove_address();
        assert_eq!(channel.address_count(), 1);
        channel.remove_address();
        assert_eq!(channel.address_count(), 0);
    }

    #[test]
    #[should_panic]
    fn remove_address_below_0() {
        let channel = InboxChannel::<()>::new(0, 1, Capacity::default());
        channel.remove_address();
    }

    #[test]
    fn adding_removing_inboxes() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default());
        assert_eq!(channel.process_count(), 1);
        channel.try_add_inbox().unwrap();
        assert_eq!(channel.process_count(), 2);
        channel.remove_inbox();
        assert_eq!(channel.process_count(), 1);
        channel.remove_inbox();
        assert_eq!(channel.process_count(), 0);
    }

    #[test]
    #[should_panic]
    fn remove_inbox_below_0() {
        let channel = InboxChannel::<()>::new(1, 0, Capacity::default());
        channel.remove_inbox();
    }

    #[test]
    fn closing() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default());
        let listeners = Listeners::size_10(&channel);

        channel.close();

        assert!(channel.is_closed());
        assert!(!channel.has_exited());
        assert_eq!(channel.push_msg(()), Err(PushError::Closed(())));
        assert_eq!(channel.pop_msg(), Err(PopError::Closed));
        listeners.assert_notified(Assert {
            recv: 10,
            exit: 0,
            send: 10,
        });
    }

    #[test]
    fn exiting() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default());
        let listeners = Listeners::size_10(&channel);

        channel.remove_inbox();

        assert!(channel.is_closed());
        assert!(channel.has_exited());
        assert_eq!(channel.process_count(), 0);
        assert_eq!(channel.address_count(), 1);
        assert_eq!(channel.push_msg(()), Err(PushError::Closed(())));
        assert_eq!(channel.pop_msg(), Err(PopError::Closed));
        listeners.assert_notified(Assert {
            recv: 10,
            exit: 10,
            send: 10,
        });
    }

    #[test]
    fn removing_all_addresses() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default());
        let listeners = Listeners::size_10(&channel);

        channel.remove_address();

        assert!(!channel.is_closed());
        assert!(!channel.has_exited());
        assert_eq!(channel.address_count(), 0);
        assert_eq!(channel.process_count(), 1);
        assert_eq!(channel.push_msg(()), Ok(()));
        listeners.assert_notified(Assert {
            recv: 1,
            exit: 0,
            send: 0,
        });
    }

    #[test]
    fn exiting_drops_all_messages() {
        let msg = Arc::new(());

        let channel = InboxChannel::new(1, 1, Capacity::Bounded(10));
        channel.send_raw_now(msg.clone()).unwrap();

        assert_eq!(Arc::strong_count(&msg), 2);
        channel.remove_inbox();
        assert_eq!(Arc::strong_count(&msg), 1);
    }

    #[test]
    fn closing_doesnt_drop_messages() {
        let channel = InboxChannel::<Arc<()>>::new(1, 1, Capacity::default());
        let msg = Arc::new(());
        channel.push_msg(msg.clone()).unwrap();
        assert_eq!(Arc::strong_count(&msg), 2);
        channel.close();
        assert_eq!(Arc::strong_count(&msg), 2);
    }

    #[tokio::test]
    async fn immedeate_halt() {
        for i in 0..100 {
            let (_child, address) = spawn(Config::default(), basic_actor!());
            spin_sleep::sleep(Duration::from_nanos(i));
            address.halt();
            address.await;
        }
    }

    #[test]
    fn add_inbox_with_0_inboxes_is_err() {
        let channel = InboxChannel::<Arc<()>>::new(1, 1, Capacity::default());
        channel.remove_inbox();
        assert_eq!(channel.try_add_inbox(), Err(()));
        assert_eq!(channel.process_count(), 0);
    }

    #[test]
    fn add_inbox_with_0_addresses_is_ok() {
        let channel = InboxChannel::<Arc<()>>::new(1, 1, Capacity::default());
        channel.remove_inbox();
        assert!(matches!(channel.try_add_inbox(), Err(_)));
        assert_eq!(channel.process_count(), 0);
    }

    #[test]
    fn push_msg() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default());
        let listeners = Listeners::size_10(&channel);

        channel.push_msg(()).unwrap();

        assert_eq!(channel.msg_count(), 1);
        listeners.assert_notified(Assert {
            recv: 1,
            exit: 0,
            send: 0,
        });
    }

    #[test]
    fn pop_msg() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default());
        channel.push_msg(()).unwrap();
        let listeners = Listeners::size_10(&channel);

        channel.pop_msg().unwrap();
        assert_eq!(channel.msg_count(), 0);
        listeners.assert_notified(Assert {
            recv: 1,
            exit: 0,
            send: 1,
        });
    }

    #[test]
    fn halt() {
        let channel = InboxChannel::<()>::new(1, 3, Capacity::default());
        let listeners = Listeners::size_10(&channel);

        channel.halt();

        assert_eq!(channel.halt_count.load(Ordering::Acquire), i32::MAX);
        listeners.assert_notified(Assert {
            recv: 10,
            exit: 0,
            send: 10,
        });
    }

    #[test]
    fn halt_closes_channel() {
        let channel = InboxChannel::<()>::new(1, 3, Capacity::default());
        channel.halt();
        assert!(channel.is_closed());
    }

    #[test]
    fn partial_halt() {
        let channel = InboxChannel::<()>::new(1, 3, Capacity::default());
        let listeners = Listeners::size_10(&channel);

        channel.halt_some(2);

        assert_eq!(channel.halt_count.load(Ordering::Acquire), 2);
        listeners.assert_notified(Assert {
            recv: 10,
            exit: 0,
            send: 0,
        });
    }

    #[test]
    fn inbox_should_halt() {
        let channel = InboxChannel::<()>::new(1, 3, Capacity::default());
        channel.halt_some(2);

        assert!(channel.inbox_should_halt());
        assert!(channel.inbox_should_halt());
        assert!(!channel.inbox_should_halt());
    }

    struct Listeners {
        recv: Vec<EventListener>,
        exit: Vec<EventListener>,
        send: Vec<EventListener>,
    }

    struct Assert {
        recv: usize,
        exit: usize,
        send: usize,
    }

    impl Listeners {
        fn size_10<T>(channel: &InboxChannel<T>) -> Self {
            Self {
                recv: (0..10)
                    .into_iter()
                    .map(|_| channel.get_recv_listener())
                    .collect(),
                exit: (0..10)
                    .into_iter()
                    .map(|_| channel.get_exit_listener())
                    .collect(),
                send: (0..10)
                    .into_iter()
                    .map(|_| channel.get_send_listener())
                    .collect(),
            }
        }

        fn assert_notified(self, assert: Assert) {
            let recv = self
                .recv
                .into_iter()
                .map(|l| l.now_or_never().is_some())
                .filter(|bool| *bool)
                .collect::<Vec<_>>()
                .len();
            let exit = self
                .exit
                .into_iter()
                .map(|l| l.now_or_never().is_some())
                .filter(|bool| *bool)
                .collect::<Vec<_>>()
                .len();
            let send = self
                .send
                .into_iter()
                .map(|l| l.now_or_never().is_some())
                .filter(|bool| *bool)
                .collect::<Vec<_>>()
                .len();

            assert_eq!(assert.recv, recv);
            assert_eq!(assert.exit, exit);
            assert_eq!(assert.send, send);
        }
    }
}
