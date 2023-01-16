use std::{
    pin::Pin,
    task::{Context, Poll},
};
use concurrent_queue::PopError;
use event_listener::EventListener;
use futures::{pin_mut, Future, FutureExt};
use tokio::task::yield_now;
use super::*;

impl<M> InboxChannel<M> {
    /// This will attempt to receive a message from the [Inbox]. If there is no message, this
    /// will return `None`.
    pub(crate) fn try_recv(&self, signaled_halt: &mut bool) -> Result<M, TryRecvError> {
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
    ) -> RecvFut<'a, M> {
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
    ) -> Option<Result<M, RecvError>> {
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
}

/// A future returned by receiving messages from an [Inbox].
///
/// This can be awaited or streamed to get the messages.
#[derive(Debug)]
pub struct RecvFut<'a, P> {
    channel: &'a InboxChannel<P>,
    signaled_halt: &'a mut bool,
    listener: &'a mut Option<EventListener>,
}

impl<'a, M> Unpin for RecvFut<'a, M> {}

impl<'a, M> Future for RecvFut<'a, M> {
    type Output = Result<M, RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self {
            channel,
            signaled_halt,
            listener,
        } = &mut *self;

        // First try to receive once, and yield if successful
        if let Some(res) = channel.poll_try_recv(signaled_halt, listener) {
            let fut = yield_now();
            pin_mut!(fut);
            let _ = fut.poll(cx);
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

#[cfg(test)]
mod test {
    use std::{future::ready, sync::Arc, time::Duration};

    use super::*;

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
