use crate::halter::CoreChannel;

use super::*;
use concurrent_queue::{ConcurrentQueue, PopError, PushError};
use event_listener::{Event, EventListener};
use futures::{future::BoxFuture, pin_mut, Future, FutureExt};
use std::{
    any::{Any, TypeId},
    fmt::Debug,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{task::yield_now, time::Sleep};
use zestors_core::{
    inboxes::Capacity,
    messaging::{
        AnyMessage, Protocol, SendCheckedError, SendError, TrySendCheckedError, TrySendError,
    },
    monitoring::{ActorId, Channel},
};

/// A [Channel] with an inbox used to receive messages.
pub struct InboxChannel<M> {
    halter_channel: CoreChannel,
    /// The underlying queue
    queue: ConcurrentQueue<M>,
    /// The capacity of the queue
    capacity: Capacity,
    /// Subscribe when trying to receive a message from this channel.
    recv_event: Event,
    /// Subscribe when trying to send a message into this channel.
    send_event: Event,
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
                Capacity::Unbounded(_) => ConcurrentQueue::unbounded(),
            },
            capacity,
            halter_channel: CoreChannel::new(address_count, inbox_count, actor_id),
            recv_event: Event::new(),
            send_event: Event::new(),
        }
    }

    pub(crate) fn empty_inbox(&self) {
        while self.pop_msg().is_ok() {}
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
    pub(crate) fn remove_process(&self) -> bool {
        if self.halter_channel.remove_process() {
            self.close();
            self.empty_inbox();
            true
        } else {
            false
        }
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
    pub(crate) fn push_msg(&self, msg: P) -> Result<(), PushError<P>> {
        match self.queue.push(msg) {
            Ok(()) => {
                self.recv_event.notify(1);
                Ok(())
            }
            Err(e) => Err(e),
        }
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
        if !(*signaled_halt) && self.halter_channel.should_halt() {
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
            listener,
        }
    }

    fn poll_try_recv(
        &self,
        signaled_halt: &mut bool,
        listener: &mut Option<EventListener>,
    ) -> Option<Result<P, RecvError>> {
        match self.try_recv(signaled_halt) {
            Ok(msg) => {
                *listener = None;
                Some(Ok(msg))
            }
            Err(signal) => match signal {
                TryRecvError::Halted => {
                    *listener = None;
                    Some(Err(RecvError::Halted))
                }
                TryRecvError::ClosedAndEmpty => {
                    *listener = None;
                    Some(Err(RecvError::ClosedAndEmpty))
                }
                TryRecvError::Empty => None,
            },
        }
    }

    pub(crate) fn send_protocol(&self, msg: P) -> SendProtocolFut<'_, P> {
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
            Capacity::Bounded(_) => self.push_msg(msg),
            Capacity::Unbounded(backoff) => match backoff.get_timeout(self.msg_count()) {
                Some(_) => return Err(TrySendError::Full(msg)),
                None => self.push_msg(msg),
            },
        }
        .map_err(|e| match e {
            PushError::Full(msg) => TrySendError::Full(msg),
            PushError::Closed(msg) => TrySendError::Closed(msg),
        })
    }

    pub(crate) fn send_protocol_blocking(&self, mut msg: P) -> Result<(), SendError<P>> {
        match self.capacity() {
            Capacity::Bounded(_) => loop {
                msg = match self.push_msg(msg) {
                    Ok(()) => {
                        return Ok(());
                    }
                    Err(PushError::Closed(msg)) => {
                        return Err(SendError(msg));
                    }
                    Err(PushError::Full(msg)) => msg,
                };

                self.get_send_listener().wait();
            },
            Capacity::Unbounded(backoff) => {
                let timeout = backoff.get_timeout(self.msg_count());
                if let Some(timeout) = timeout {
                    std::thread::sleep(timeout);
                }
                self.push_msg(msg).map_err(|e| match e {
                    PushError::Full(_) => unreachable!("unbounded"),
                    PushError::Closed(msg) => SendError(msg),
                })
            }
        }
    }
}

impl<P: Protocol> Channel for InboxChannel<P> {
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
        self.halter_channel.halt_some(n);
        self.recv_event.notify(usize::MAX)
    }

    /// Returns the amount of inboxes this channel has.
    fn process_count(&self) -> usize {
        self.halter_channel.process_count()
    }

    /// Returns the amount of messages currently in the channel.
    fn msg_count(&self) -> usize {
        self.queue.len()
    }

    /// Returns the amount of addresses this channel has.
    fn address_count(&self) -> usize {
        self.halter_channel.address_count()
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
        self.halter_channel.has_exited()
    }

    /// Add an Address to the channel, incrementing address-count by 1. Afterwards,
    /// a new Address may be created from this channel.
    ///
    /// Returns the previous inbox-count
    fn add_address(&self) -> usize {
        self.halter_channel.add_address()
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
        self.halter_channel.remove_address()
    }

    fn get_exit_listener(&self) -> EventListener {
        self.halter_channel.get_exit_listener()
    }

    /// Get the actor_id.
    fn actor_id(&self) -> ActorId {
        self.halter_channel.actor_id()
    }

    // Closes the channel and halts all actors.
    fn halt(&self) {
        self.close();
        self.halt_some(u32::MAX);
    }

    fn try_add_inbox(&self) -> Result<usize, ()> {
        self.halter_channel.try_add_inbox()
    }

    fn try_send_boxed(&self, boxed: AnyMessage) -> Result<(), TrySendCheckedError<AnyMessage>> {
        match P::try_from_msg(boxed) {
            Ok(prot) => self.try_send_protocol(prot).map_err(|e| match e {
                TrySendError::Full(prot) => TrySendCheckedError::Full(prot.into_msg()),
                TrySendError::Closed(prot) => TrySendCheckedError::Closed(prot.into_msg()),
            }),
            Err(boxed) => Err(TrySendCheckedError::NotAccepted(boxed)),
        }
    }

    fn send_now_boxed(&self, boxed: AnyMessage) -> Result<(), TrySendCheckedError<AnyMessage>> {
        match P::try_from_msg(boxed) {
            Ok(prot) => self.send_protocol_now(prot).map_err(|e| match e {
                TrySendError::Full(prot) => TrySendCheckedError::Full(prot.into_msg()),
                TrySendError::Closed(prot) => TrySendCheckedError::Closed(prot.into_msg()),
            }),
            Err(boxed) => Err(TrySendCheckedError::NotAccepted(boxed)),
        }
    }

    fn send_blocking_boxed(&self, boxed: AnyMessage) -> Result<(), SendCheckedError<AnyMessage>> {
        match P::try_from_msg(boxed) {
            Ok(prot) => self
                .send_protocol_blocking(prot)
                .map_err(|SendError(prot)| SendCheckedError::Closed(prot.into_msg())),
            Err(boxed) => Err(SendCheckedError::NotAccepted(boxed)),
        }
    }

    fn send_boxed(
        &self,
        boxed: AnyMessage,
    ) -> BoxFuture<'_, Result<(), SendCheckedError<AnyMessage>>> {
        Box::pin(async move {
            match P::try_from_msg(boxed) {
                Ok(prot) => self
                    .send_protocol(prot)
                    .await
                    .map_err(|SendError(prot)| SendCheckedError::Closed(prot.into_msg())),
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
        f.debug_struct("InboxChannel")
            .field("halter_channel", &self.halter_channel)
            .field("queue", &self.queue)
            .field("capacity", &self.capacity)
            .field("recv_event", &self.recv_event)
            .field("send_event", &self.send_event)
            .finish()
    }
}

//------------------------------------------------------------------------------------------------
//  RecvFut
//------------------------------------------------------------------------------------------------

/// A future returned by receiving messages from an [Inbox].
///
/// This can be awaited or streamed to get the messages.
#[derive(Debug)]
pub struct RecvFut<'a, P> {
    channel: &'a InboxChannel<P>,
    signaled_halt: &'a mut bool,
    listener: &'a mut Option<EventListener>,
}

impl<'a, P: Protocol> Unpin for RecvFut<'a, P> {}

impl<'a, P: Protocol> Future for RecvFut<'a, P> {
    type Output = Result<P, RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self {
            channel,
            signaled_halt,
            listener,
        } = &mut *self;

        // First try to receive once, and yield if successful
        if let Some(res) = channel.poll_try_recv(signaled_halt, listener) {
            return Poll::Ready(res);
        }

        loop {
            // Otherwise, acquire a listener, if we don't have one yet
            if listener.is_none() {
                **listener = Some(channel.get_recv_listener())
            }

            // Attempt to receive a message, and return if ready
            if let Some(res) = channel.poll_try_recv(signaled_halt, listener) {
                return Poll::Ready(res);
            }

            // And poll the listener
            match listener.as_mut().unwrap().poll_unpin(cx) {
                Poll::Ready(()) => {
                    **listener = None;
                    // Attempt to receive a message, and return if ready
                    if let Some(res) = channel.poll_try_recv(signaled_halt, listener) {
                        return Poll::Ready(res);
                    }
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl<'a, M> Drop for RecvFut<'a, M> {
    fn drop(&mut self) {
        *self.listener = None;
    }
}

//------------------------------------------------------------------------------------------------
//  SendRawFut
//------------------------------------------------------------------------------------------------

/// The send-future, this can be `.await`-ed to send the message.
#[derive(Debug)]
pub(crate) struct SendProtocolFut<'a, M> {
    channel: &'a InboxChannel<M>,
    msg: Option<M>,
    fut: Option<InnerSendFut>,
}

/// Listener for a bounded channel, sleep for an unbounded channel.
#[derive(Debug)]
enum InnerSendFut {
    Listener(EventListener),
    Sleep(Pin<Box<Sleep>>),
}

impl Unpin for InnerSendFut {}
impl Future for InnerSendFut {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &mut *self {
            InnerSendFut::Listener(listener) => listener.poll_unpin(cx),
            InnerSendFut::Sleep(sleep) => sleep.poll_unpin(cx),
        }
    }
}

impl<'a, P: Protocol> SendProtocolFut<'a, P> {
    pub(crate) fn new(channel: &'a InboxChannel<P>, msg: P) -> Self {
        match channel.capacity() {
            Capacity::Bounded(_) => SendProtocolFut {
                channel,
                msg: Some(msg),
                fut: None,
            },
            Capacity::Unbounded(back_pressure) => SendProtocolFut {
                channel,
                msg: Some(msg),
                fut: back_pressure
                    .get_timeout(channel.msg_count())
                    .map(|timeout| InnerSendFut::Sleep(Box::pin(tokio::time::sleep(timeout)))),
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
                self.fut = Some(InnerSendFut::Listener(self.channel.get_send_listener()))
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
            Capacity::Unbounded(_) => self.poll_unbounded_send(cx),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{sync::Arc, time::Duration};
    use tokio::time::Instant;
    use zestors_core::inboxes::BackPressure;

    #[test]
    fn try_send_with_space() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::Bounded(10), ActorId::generate());
        channel.try_send_protocol(()).unwrap();
        channel.send_protocol_now(()).unwrap();
        assert_eq!(channel.msg_count(), 2);

        let channel = InboxChannel::<()>::new(
            1,
            1,
            Capacity::Unbounded(BackPressure::disabled()),
            ActorId::generate(),
        );
        channel.try_send_protocol(()).unwrap();
        channel.send_protocol_now(()).unwrap();
        assert_eq!(channel.msg_count(), 2);
    }

    #[test]
    fn try_send_unbounded_full() {
        let channel = InboxChannel::<()>::new(
            1,
            1,
            Capacity::Unbounded(BackPressure::linear(0, Duration::from_secs(1))),
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

        let channel = InboxChannel::<()>::new(
            1,
            1,
            Capacity::Unbounded(BackPressure::disabled()),
            ActorId::generate(),
        );
        channel.send_protocol(()).await.unwrap();
        assert_eq!(channel.msg_count(), 1);
    }

    #[tokio::test]
    async fn send_unbounded_full() {
        let channel = InboxChannel::<()>::new(
            1,
            1,
            Capacity::Unbounded(BackPressure::linear(0, Duration::from_millis(1))),
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
            let time = tokio::time::Instant::now();
            channel_clone.send_protocol(()).await.unwrap();
            channel_clone.send_protocol(()).await.unwrap();
            channel_clone.send_protocol(()).await.unwrap();
            assert!(time.elapsed().as_millis() > 5);
        });

        channel.recv(&mut false, &mut None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        channel.recv(&mut false, &mut None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        channel.recv(&mut false, &mut None).await.unwrap();
    }
}

#[cfg(test)]
mod test3 {
    use super::*;
    use crate::inbox::{RecvError, TryRecvError};
    use std::{future::ready, sync::Arc, time::Duration};

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
}

#[cfg(test)]
mod test2 {
    use std::{
        sync::{atomic::Ordering, Arc},
        time::Duration,
    };

    use super::*;
    use concurrent_queue::{PopError, PushError};
    use event_listener::EventListener;
    use futures::FutureExt;
    use zestors_core::inboxes::BackPressure;

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
            Capacity::Unbounded(BackPressure::default()),
            ActorId::generate(),
        );
        assert!(channel.queue.capacity().is_none());
        assert!(!channel.capacity().is_bounded());
    }

    #[test]
    fn adding_removing_addresses() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
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
        let channel = InboxChannel::<()>::new(0, 1, Capacity::default(), ActorId::generate());
        channel.remove_address();
    }

    #[test]
    fn adding_removing_inboxes() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
        assert_eq!(channel.process_count(), 1);
        channel.try_add_inbox().unwrap();
        assert_eq!(channel.process_count(), 2);
        channel.remove_process();
        assert_eq!(channel.process_count(), 1);
        channel.remove_process();
        assert_eq!(channel.process_count(), 0);
    }

    #[test]
    #[should_panic]
    fn remove_inbox_below_0() {
        let channel = InboxChannel::<()>::new(1, 0, Capacity::default(), ActorId::generate());
        channel.remove_process();
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

        channel.remove_process();

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

        let channel = InboxChannel::new(1, 1, Capacity::Bounded(10), ActorId::generate());
        channel.send_protocol_now(msg.clone()).unwrap();

        assert_eq!(Arc::strong_count(&msg), 2);
        channel.remove_process();
        assert_eq!(Arc::strong_count(&msg), 1);
    }

    #[test]
    fn closing_doesnt_drop_messages() {
        let channel = InboxChannel::<Arc<()>>::new(1, 1, Capacity::default(), ActorId::generate());
        let msg = Arc::new(());
        channel.push_msg(msg.clone()).unwrap();
        assert_eq!(Arc::strong_count(&msg), 2);
        channel.close();
        assert_eq!(Arc::strong_count(&msg), 2);
    }

    #[test]
    fn add_inbox_with_0_inboxes_is_err() {
        let channel = InboxChannel::<Arc<()>>::new(1, 1, Capacity::default(), ActorId::generate());
        channel.remove_process();
        assert_eq!(channel.try_add_inbox(), Err(()));
        assert_eq!(channel.process_count(), 0);
    }

    #[test]
    fn add_inbox_with_0_addresses_is_ok() {
        let channel = InboxChannel::<Arc<()>>::new(1, 1, Capacity::default(), ActorId::generate());
        channel.remove_process();
        assert!(matches!(channel.try_add_inbox(), Err(_)));
        assert_eq!(channel.process_count(), 0);
    }

    #[test]
    fn push_msg() {
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
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
        let channel = InboxChannel::<()>::new(1, 1, Capacity::default(), ActorId::generate());
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
        let channel = InboxChannel::<()>::new(1, 3, Capacity::default(), ActorId::generate());
        let listeners = Listeners::size_10(&channel);

        channel.halt();

        assert_eq!(channel.halter_channel.halt_count(), i32::MAX);
        listeners.assert_notified(Assert {
            recv: 10,
            exit: 0,
            send: 10,
        });
    }

    #[test]
    fn halt_closes_channel() {
        let channel = InboxChannel::<()>::new(1, 3, Capacity::default(), ActorId::generate());
        channel.halt();
        assert!(channel.is_closed());
    }

    #[test]
    fn partial_halt() {
        let channel = InboxChannel::<()>::new(1, 3, Capacity::default(), ActorId::generate());
        let listeners = Listeners::size_10(&channel);

        channel.halt_some(2);

        assert_eq!(channel.halter_channel.halt_count(), 2);
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

        assert!(channel.halter_channel.should_halt());
        assert!(channel.halter_channel.should_halt());
        assert!(!channel.halter_channel.should_halt());
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
