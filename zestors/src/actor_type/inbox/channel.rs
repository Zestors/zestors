use super::*;
use concurrent_queue::{ConcurrentQueue, PopError, PushError};
use event_listener::{Event, EventListener};
use futures::{future::BoxFuture, Future, FutureExt};
use std::{
    any::{Any, TypeId},
    fmt::Debug,
    pin::Pin,
    sync::{
        atomic::{AtomicI32, AtomicUsize, Ordering},
        Arc,
    },
    task::{ready, Context, Poll},
};
use tokio::time::Sleep;

/// A [Channel] with an inbox used to receive messages.
pub struct InboxChannel<P> {
    /// The underlying queue
    queue: ConcurrentQueue<P>,
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

impl<P: Protocol> InboxChannel<P> {
    /// Create a new channel, given an address count, inbox_count and capacity.
    ///
    /// After this, it must be ensured that the correct amount of inboxes and addresses actually exist.
    pub(crate) fn new(
        address_count: usize,
        inbox_count: usize,
        capacity: Capacity,
        actor_id: ActorId,
    ) -> Self {
        Self {
            queue: match &capacity {
                Capacity::Bounded(size) => ConcurrentQueue::bounded(size.to_owned()),
                Capacity::BackPressure(_) | Capacity::Unbounded => ConcurrentQueue::unbounded(),
            },
            capacity,
            address_count: AtomicUsize::new(address_count),
            inbox_count: AtomicUsize::new(inbox_count),
            recv_event: Event::new(),
            send_event: Event::new(),
            exit_event: Event::new(),
            halt_count: AtomicI32::new(0),
            actor_id,
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
    pub(crate) fn pop_msg(&self) -> Result<P, PopError> {
        self.queue.pop().map(|msg| {
            self.send_event.notify(usize::MAX);
            self.recv_event.notify(usize::MAX);
            msg
        })
    }

    /// Push a message into the channel.
    ///
    /// Can fail either because the queue is full, or because it is closed.
    ///
    /// ## Notifies
    /// on success -> 1 recv_listener
    pub(crate) fn push_msg(&self, msg: P) -> Result<(), PushError<P>> {
        match self.queue.push(msg) {
            Ok(()) => {
                self.recv_event.notify(usize::MAX);
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

    /// This will attempt to receive a message from the [Inbox]. If there is no message, this
    /// will return `None`.
    pub(crate) fn try_recv(&self, signaled_halt: &mut bool) -> Result<P, TryRecvError> {
        if !(*signaled_halt) && self.inbox_should_halt() {
            *signaled_halt = true;
            Err(TryRecvError::Halted)
        } else {
            self.pop_msg().map_err(|e| match e {
                PopError::Empty => TryRecvError::Empty,
                PopError::Closed => TryRecvError::ClosedAndEmpty,
            })
        }
    }

    /// Wait until there is a message in the [Inbox].
    pub(crate) fn recv<'a>(
        &'a self,
        signaled_halt: &'a mut bool,
        listener: &'a mut Option<EventListener>,
    ) -> RecvFut<'a, P> {
        RecvFut {
            channel: self,
            signaled_halt,
            recv_listener: listener,
        }
    }

    pub(super) fn send_protocol(&self, msg: P) -> SendProtocolFut<'_, P> {
        SendProtocolFut::new(self, msg)
    }

    pub(crate) fn send_protocol_now(&self, msg: P) -> Result<(), TrySendError<P>> {
        self.push_msg(msg).map_err(|e| match e {
            PushError::Full(msg) => TrySendError::Full(msg),
            PushError::Closed(msg) => TrySendError::Closed(msg),
        })
    }

    pub(crate) fn try_send_protocol(&self, msg: P) -> Result<(), TrySendError<P>> {
        match self.capacity() {
            Capacity::Bounded(_) | Capacity::Unbounded => self.push_msg(msg),
            Capacity::BackPressure(backoff) => match backoff.get_timeout(self.msg_count()) {
                Some(_) => return Err(TrySendError::Full(msg)),
                None => self.push_msg(msg),
            },
        }
        .map_err(|e| match e {
            PushError::Full(msg) => TrySendError::Full(msg),
            PushError::Closed(msg) => TrySendError::Closed(msg),
        })
    }

    pub(crate) fn send_protocol_blocking(&self, msg: P) -> Result<(), SendError<P>> {
        futures::executor::block_on(self.send_protocol(msg))
    }
}

impl<P: Protocol> Channel for InboxChannel<P> {
    fn close(&self) -> bool {
        if self.queue.close() {
            self.recv_event.notify(usize::MAX);
            self.send_event.notify(usize::MAX);
            true
        } else {
            false
        }
    }

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

    fn process_count(&self) -> usize {
        self.inbox_count.load(Ordering::Acquire)
    }

    fn msg_count(&self) -> usize {
        self.queue.len()
    }

    fn address_count(&self) -> usize {
        self.address_count.load(Ordering::Acquire)
    }

    fn is_closed(&self) -> bool {
        self.queue.is_closed()
    }

    fn capacity(&self) -> Capacity {
        self.capacity.clone()
    }

    fn has_exited(&self) -> bool {
        self.inbox_count.load(Ordering::Acquire) == 0
    }

    fn increment_address_count(&self) -> usize {
        self.address_count.fetch_add(1, Ordering::AcqRel)
    }

    fn decrement_address_count(&self) -> usize {
        let prev_address_count = self.address_count.fetch_sub(1, Ordering::AcqRel);
        assert!(prev_address_count >= 1);
        prev_address_count
    }

    fn get_exit_listener(&self) -> EventListener {
        self.exit_event.listen()
    }

    fn actor_id(&self) -> ActorId {
        self.actor_id
    }

    fn halt(&self) {
        self.halt_some(u32::MAX);
    }

    fn try_increment_process_count(&self) -> Result<usize, AddProcessError> {
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
            Err(_) => Err(AddProcessError::ActorHasExited),
        }
    }

    fn try_send_box(&self, boxed: BoxPayload) -> Result<(), TrySendCheckedError<BoxPayload>> {
        match P::try_from_boxed_payload(boxed) {
            Ok(prot) => self.try_send_protocol(prot).map_err(|e| match e {
                TrySendError::Full(prot) => TrySendCheckedError::Full(prot.into_boxed_payload()),
                TrySendError::Closed(prot) => {
                    TrySendCheckedError::Closed(prot.into_boxed_payload())
                }
            }),
            Err(boxed) => Err(TrySendCheckedError::NotAccepted(boxed)),
        }
    }

    fn force_send_box(&self, boxed: BoxPayload) -> Result<(), TrySendCheckedError<BoxPayload>> {
        match P::try_from_boxed_payload(boxed) {
            Ok(prot) => self.send_protocol_now(prot).map_err(|e| match e {
                TrySendError::Full(prot) => TrySendCheckedError::Full(prot.into_boxed_payload()),
                TrySendError::Closed(prot) => {
                    TrySendCheckedError::Closed(prot.into_boxed_payload())
                }
            }),
            Err(boxed) => Err(TrySendCheckedError::NotAccepted(boxed)),
        }
    }

    fn send_box_blocking(&self, boxed: BoxPayload) -> Result<(), SendCheckedError<BoxPayload>> {
        match P::try_from_boxed_payload(boxed) {
            Ok(prot) => self
                .send_protocol_blocking(prot)
                .map_err(|SendError(prot)| SendCheckedError::Closed(prot.into_boxed_payload())),
            Err(boxed) => Err(SendCheckedError::NotAccepted(boxed)),
        }
    }

    fn send_box(
        &self,
        boxed: BoxPayload,
    ) -> BoxFuture<'_, Result<(), SendCheckedError<BoxPayload>>> {
        Box::pin(async move {
            match P::try_from_boxed_payload(boxed) {
                Ok(prot) => self
                    .send_protocol(prot)
                    .await
                    .map_err(|SendError(prot)| SendCheckedError::Closed(prot.into_boxed_payload())),
                Err(boxed) => Err(SendCheckedError::NotAccepted(boxed)),
            }
        })
    }

    fn accepts(&self, id: &TypeId) -> bool {
        <P as Protocol>::accepts_msg(id)
    }

    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn into_dyn(self: Arc<Self>) -> Arc<dyn Channel> {
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

//------------------------------------------------------------------------------------------------
//  RecvFut
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct RecvFut<'a, P> {
    channel: &'a InboxChannel<P>,
    signaled_halt: &'a mut bool,
    recv_listener: &'a mut Option<EventListener>,
}

impl<'a, P: Protocol> Unpin for RecvFut<'a, P> {}

impl<'a, P: Protocol> Future for RecvFut<'a, P> {
    type Output = Result<P, RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        let result = loop {
            let recv_listener = this
                .recv_listener
                .get_or_insert(this.channel.get_recv_listener());

            match this.channel.try_recv(&mut this.signaled_halt) {
                Ok(msg) => break Poll::Ready(Ok(msg)),
                Err(error) => match error {
                    TryRecvError::Halted => break Poll::Ready(Err(RecvError::Halted)),
                    TryRecvError::ClosedAndEmpty => {
                        break Poll::Ready(Err(RecvError::ClosedAndEmpty))
                    }
                    TryRecvError::Empty => {
                        ready!(recv_listener.poll_unpin(cx));
                        *this.recv_listener = None;
                    }
                },
            };
        };

        if result.is_ready() {
            *this.recv_listener = None;
        }
        result
    }
}

impl<'a, M> Drop for RecvFut<'a, M> {
    fn drop(&mut self) {
        *self.recv_listener = None;
    }
}

//------------------------------------------------------------------------------------------------
//  SendProtocolFut
//------------------------------------------------------------------------------------------------

/// The send-future, this can be `.await`-ed to send the message.
#[derive(Debug)]
pub(super) struct SendProtocolFut<'a, M> {
    channel: &'a InboxChannel<M>,
    msg: Option<M>,
    fut: Option<InnerSendProtocolFut>,
}

/// Listener for a bounded channel, sleep for an unbounded channel.
#[derive(Debug)]
enum InnerSendProtocolFut {
    Listener(EventListener),
    Sleep(Pin<Box<Sleep>>),
}

impl Unpin for InnerSendProtocolFut {}
impl Future for InnerSendProtocolFut {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &mut *self {
            InnerSendProtocolFut::Listener(listener) => listener.poll_unpin(cx),
            InnerSendProtocolFut::Sleep(sleep) => sleep.poll_unpin(cx),
        }
    }
}

impl<'a, P: Protocol> SendProtocolFut<'a, P> {
    pub(crate) fn new(channel: &'a InboxChannel<P>, msg: P) -> Self {
        match channel.capacity() {
            Capacity::Bounded(_) | Capacity::Unbounded => SendProtocolFut {
                channel,
                msg: Some(msg),
                fut: None,
            },
            Capacity::BackPressure(back_pressure) => SendProtocolFut {
                channel,
                msg: Some(msg),
                fut: back_pressure
                    .get_timeout(channel.msg_count())
                    .map(|timeout| {
                        InnerSendProtocolFut::Sleep(Box::pin(tokio::time::sleep(timeout)))
                    }),
            },
        }
    }

    fn poll_bounded_send(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), SendError<P>>> {
        macro_rules! try_send {
            ($msg:ident) => {
                match self.channel.try_send_protocol($msg) {
                    Ok(()) => return Poll::Ready(Ok(())),
                    Err(e) => match e {
                        TrySendError::Closed(msg) => return Poll::Ready(Err(SendError(msg))),
                        TrySendError::Full(msg_new) => $msg = msg_new,
                    },
                }
            };
        }

        let mut msg = self.msg.take().unwrap();

        try_send!(msg);

        loop {
            // Otherwise, we create the future if it doesn't exist yet.
            if self.fut.is_none() {
                self.fut = Some(InnerSendProtocolFut::Listener(
                    self.channel.get_send_listener(),
                ))
            }

            try_send!(msg);

            // Poll it once, and return if pending, otherwise we loop again.
            match self.fut.as_mut().unwrap().poll_unpin(cx) {
                Poll::Ready(()) => {
                    try_send!(msg);
                    self.fut = None
                }
                Poll::Pending => {
                    self.msg = Some(msg);
                    return Poll::Pending;
                }
            }
        }
    }

    fn poll_unbounded_send(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), SendError<P>>> {
        if let Some(fut) = &mut self.fut {
            match fut.poll_unpin(cx) {
                Poll::Ready(()) => self.poll_push_unbounded(),
                Poll::Pending => Poll::Pending,
            }
        } else {
            self.poll_push_unbounded()
        }
    }

    fn poll_push_unbounded(&mut self) -> Poll<Result<(), SendError<P>>> {
        let msg = self.msg.take().unwrap();
        match self.channel.push_msg(msg) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(PushError::Closed(msg)) => Poll::Ready(Err(SendError(msg))),
            Err(PushError::Full(_msg)) => unreachable!(),
        }
    }
}

impl<'a, M> Unpin for SendProtocolFut<'a, M> {}

impl<'a, P: Protocol> Future for SendProtocolFut<'a, P> {
    type Output = Result<(), SendError<P>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.channel.capacity() {
            Capacity::Bounded(_) => self.poll_bounded_send(cx),
            Capacity::BackPressure(_) | Capacity::Unbounded => self.poll_unbounded_send(cx),
        }
    }
}

//------------------------------------------------------------------------------------------------
//  Test
//------------------------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use crate as zestors;
    use crate::inbox::BackPressure;
    use crate::inbox::{RecvError, TryRecvError};
    use std::future::ready;
    use std::{sync::Arc, time::Duration};
    use tokio::time::Instant;
    #[protocol]
    #[derive(Debug)]
    enum ArcProtocol {
        Msg(Arc<()>),
    }

    #[test]
    fn try_send_with_space() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::Bounded(10), ActorId::generate());
        channel.try_send_protocol(()).unwrap();
        channel.send_protocol_now(()).unwrap();
        assert_eq!(channel.msg_count(), 2);

        let channel = InboxChannel::<()>::new(1, 1, Capacity::Unbounded, ActorId::generate());
        channel.try_send_protocol(()).unwrap();
        channel.send_protocol_now(()).unwrap();
        assert_eq!(channel.msg_count(), 2);
    }

    #[test]
    fn try_send_unbounded_full() {
        let channel = InboxChannel::<()>::new(
            1,
            1,
            Capacity::BackPressure(BackPressure::linear(0, Duration::from_secs(1))),
            ActorId::generate(),
        );
        assert_eq!(channel.try_send_protocol(()), Err(TrySendError::Full(())));
        assert_eq!(channel.send_protocol_now(()), Ok(()));
        assert_eq!(channel.msg_count(), 1);
    }

    #[test]
    fn try_send_bounded_full() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::Bounded(1), ActorId::generate());
        channel.try_send_protocol(()).unwrap();
        assert_eq!(channel.try_send_protocol(()), Err(TrySendError::Full(())));
        assert_eq!(channel.send_protocol_now(()), Err(TrySendError::Full(())));
        assert_eq!(channel.msg_count(), 1);
    }

    #[tokio::test]
    async fn send_with_space() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::Bounded(10), ActorId::generate());
        channel.send_protocol(()).await.unwrap();
        assert_eq!(channel.msg_count(), 1);

        let channel = InboxChannel::<()>::new(1, 1, Capacity::Unbounded, ActorId::generate());
        channel.send_protocol(()).await.unwrap();
        assert_eq!(channel.msg_count(), 1);
    }

    #[tokio::test]
    async fn send_unbounded_full() {
        let channel = InboxChannel::<()>::new(
            1,
            1,
            Capacity::BackPressure(BackPressure::linear(0, Duration::from_millis(1))),
            ActorId::generate(),
        );
        let time = Instant::now();
        channel.send_protocol(()).await.unwrap();
        channel.send_protocol(()).await.unwrap();
        channel.send_protocol(()).await.unwrap();
        assert!(time.elapsed().as_millis() > 6);
        assert_eq!(channel.msg_count(), 3);
    }

    #[tokio::test]
    async fn send_bounded_full() {
        let channel = Arc::new(InboxChannel::<()>::new(
            1,
            1,
            Capacity::Bounded(1),
            ActorId::generate(),
        ));
        let channel_clone = channel.clone();

        tokio::task::spawn(async move {
            let time = Instant::now();
            channel_clone.send_protocol(()).await.unwrap();
            channel_clone.send_protocol(()).await.unwrap();
            channel_clone.send_protocol(()).await.unwrap();
            assert!(time.elapsed().as_millis() > 2);
        });

        channel.recv(&mut false, &mut None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(2)).await;
        channel.recv(&mut false, &mut None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(2)).await;
        channel.recv(&mut false, &mut None).await.unwrap();
    }

    #[test]
    fn try_recv() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
        channel.push_msg(()).unwrap();
        channel.push_msg(()).unwrap();

        assert!(channel.try_recv(&mut true).is_ok());
        assert!(channel.try_recv(&mut false).is_ok());
        assert_eq!(channel.try_recv(&mut true), Err(TryRecvError::Empty));
        assert_eq!(channel.try_recv(&mut false), Err(TryRecvError::Empty));
    }

    #[test]
    fn try_recv_closed() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
        channel.push_msg(()).unwrap();
        channel.push_msg(()).unwrap();
        channel.close();

        assert!(channel.try_recv(&mut true).is_ok());
        assert!(channel.try_recv(&mut false).is_ok());
        assert_eq!(
            channel.try_recv(&mut true),
            Err(TryRecvError::ClosedAndEmpty)
        );
        assert_eq!(
            channel.try_recv(&mut false),
            Err(TryRecvError::ClosedAndEmpty)
        );
    }

    #[test]
    fn try_recv_halt() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
        channel.push_msg(()).unwrap();
        channel.push_msg(()).unwrap();
        channel.halt_some(1);

        assert_eq!(channel.try_recv(&mut false), Err(TryRecvError::Halted));
        assert!(channel.try_recv(&mut true).is_ok());
        assert!(channel.try_recv(&mut false).is_ok());
        assert_eq!(channel.try_recv(&mut true), Err(TryRecvError::Empty));
        assert_eq!(channel.try_recv(&mut false), Err(TryRecvError::Empty));
    }

    #[tokio::test]
    async fn recv_immedeate() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
        let mut listener = None;
        channel.push_msg(()).unwrap();
        channel.close();

        assert_eq!(channel.recv(&mut false, &mut listener).await, Ok(()));
        assert_eq!(
            channel.recv(&mut false, &mut listener).await,
            Err(RecvError::ClosedAndEmpty)
        );
    }

    #[tokio::test]
    async fn recv_delayed() {
        let channel = Arc::new(InboxChannel::<()>::new(
            1,
            1,
            Capacity::default(),
            ActorId::generate(),
        ));
        let channel_clone = channel.clone();

        let handle = tokio::task::spawn(async move {
            let mut listener = None;
            assert_eq!(channel_clone.recv(&mut false, &mut listener).await, Ok(()));
            assert_eq!(
                channel_clone.recv(&mut false, &mut listener).await,
                Err(RecvError::ClosedAndEmpty)
            );
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        channel.push_msg(()).unwrap();
        channel.close();
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn dropping_recv_notifies_next() {
        let channel = Arc::new(InboxChannel::<()>::new(
            1,
            1,
            Capacity::default(),
            ActorId::generate(),
        ));
        let channel_clone = channel.clone();

        let handle = tokio::task::spawn(async move {
            let mut listener = None;
            let mut halt = false;
            let mut recv1 = channel_clone.recv(&mut halt, &mut listener);
            tokio::select! {
                biased;
                _ = &mut recv1 => {
                    panic!()
                }
                _ = ready(||()) => {
                    ()
                }
            }
            let mut listener = None;
            let mut halt = false;
            let recv2 = channel_clone.recv(&mut halt, &mut listener);
            drop(recv1);
            recv2.await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        channel.push_msg(()).unwrap();
        channel.close();
        handle.await.unwrap();
    }

    #[test]
    fn channels_have_actor_ids() {
        let id1 =
            InboxChannel::<()>::new(1, 1, Capacity::Bounded(10), ActorId::generate()).actor_id();
        let id2 =
            InboxChannel::<()>::new(1, 1, Capacity::Bounded(10), ActorId::generate()).actor_id();
        assert!(id1 < id2);
    }

    #[test]
    fn capacity_types_are_correct() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::Bounded(10), ActorId::generate());
        assert!(channel.queue.capacity().is_some());
        assert!(channel.capacity().is_bounded());
        let channel = InboxChannel::<()>::new(
            1,
            1,
            Capacity::BackPressure(BackPressure::default()),
            ActorId::generate(),
        );
        assert!(channel.queue.capacity().is_none());
        assert!(!channel.capacity().is_bounded());
    }

    #[test]
    fn adding_removing_addresses() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
        assert_eq!(channel.address_count(), 1);
        channel.increment_address_count();
        assert_eq!(channel.address_count(), 2);
        channel.decrement_address_count();
        assert_eq!(channel.address_count(), 1);
        channel.decrement_address_count();
        assert_eq!(channel.address_count(), 0);
    }

    #[test]
    #[should_panic]
    fn remove_address_below_0() {
        let channel = InboxChannel::<()>::new(0, 1, Capacity::default(), ActorId::generate());
        channel.decrement_address_count();
    }

    #[test]
    fn adding_removing_inboxes() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
        assert_eq!(channel.process_count(), 1);
        channel.try_increment_process_count().unwrap();
        assert_eq!(channel.process_count(), 2);
        channel.remove_inbox();
        assert_eq!(channel.process_count(), 1);
        channel.remove_inbox();
        assert_eq!(channel.process_count(), 0);
    }

    #[test]
    #[should_panic]
    fn remove_inbox_below_0() {
        let channel = InboxChannel::<()>::new(1, 0, Capacity::default(), ActorId::generate());
        channel.remove_inbox();
    }

    #[test]
    fn closing() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
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
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
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
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
        let listeners = Listeners::size_10(&channel);

        channel.decrement_address_count();

        assert!(!channel.is_closed());
        assert!(!channel.has_exited());
        assert_eq!(channel.address_count(), 0);
        assert_eq!(channel.process_count(), 1);
        assert_eq!(channel.push_msg(()), Ok(()));
        listeners.assert_notified(Assert {
            recv: 10,
            exit: 0,
            send: 0,
        });
    }

    #[test]
    fn exiting_drops_all_messages() {
        let msg = Arc::new(());

        let channel = InboxChannel::new(1, 1, Capacity::Bounded(10), ActorId::generate());
        channel
            .send_protocol_now(ArcProtocol::Msg(msg.clone()))
            .unwrap();

        assert_eq!(Arc::strong_count(&msg), 2);
        channel.remove_inbox();
        assert_eq!(Arc::strong_count(&msg), 1);
    }

    #[test]
    fn closing_doesnt_drop_messages() {
        let channel = InboxChannel::new(1, 1, Capacity::default(), ActorId::generate());
        let msg = Arc::new(());
        channel.push_msg(ArcProtocol::Msg(msg.clone())).unwrap();
        assert_eq!(Arc::strong_count(&msg), 2);
        channel.close();
        assert_eq!(Arc::strong_count(&msg), 2);
    }

    #[test]
    fn add_inbox_with_0_inboxes_is_err() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
        channel.remove_inbox();
        assert_eq!(
            channel.try_increment_process_count(),
            Err(AddProcessError::ActorHasExited)
        );
        assert_eq!(channel.process_count(), 0);
    }

    #[test]
    fn add_inbox_with_0_addresses_is_ok() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
        channel.remove_inbox();
        assert!(matches!(channel.try_increment_process_count(), Err(_)));
        assert_eq!(channel.process_count(), 0);
    }

    #[test]
    fn push_msg() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
        let listeners = Listeners::size_10(&channel);

        channel.push_msg(()).unwrap();

        assert_eq!(channel.msg_count(), 1);
        listeners.assert_notified(Assert {
            recv: 10,
            exit: 0,
            send: 0,
        });
    }

    #[test]
    fn pop_msg() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
        channel.push_msg(()).unwrap();
        let listeners = Listeners::size_10(&channel);

        channel.pop_msg().unwrap();
        assert_eq!(channel.msg_count(), 0);
        listeners.assert_notified(Assert {
            recv: 10,
            exit: 0,
            send: 10,
        });
    }

    #[test]
    fn halt() {
        let channel = InboxChannel::<()>::new(1, 3, Capacity::default(), ActorId::generate());
        let listeners = Listeners::size_10(&channel);

        channel.halt();

        assert_eq!(channel.halt_count.load(Ordering::Acquire), i32::MAX);
        listeners.assert_notified(Assert {
            recv: 10,
            exit: 0,
            send: 0,
        });
    }

    #[test]
    fn halt_doesnt_close_channel() {
        let channel = InboxChannel::<()>::new(1, 3, Capacity::default(), ActorId::generate());
        channel.halt();
        assert!(!channel.is_closed());
    }

    #[test]
    fn partial_halt() {
        let channel = InboxChannel::<()>::new(1, 3, Capacity::default(), ActorId::generate());
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
        let channel = InboxChannel::<()>::new(1, 3, Capacity::default(), ActorId::generate());
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
        fn size_10<T: Protocol>(channel: &InboxChannel<T>) -> Self {
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
