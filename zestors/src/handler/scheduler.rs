use crate::all::*;
use futures::{stream::FuturesUnordered, Future, Stream, StreamExt};
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

/// A [`Scheduler`] is a [`HandlerState`] that allows you to schedule multiple futures and a
/// single stream to be run concurrently. The parameter `T` can be any other handler-state such
/// as an [`Inbox<P>`], and the parameter `H` is the [`Handler`] that uses the state.
///
/// The order in which items are processed is:
/// 1 From the state `T`.
/// 2. From any scheduled futures.
/// 3. From the scheduled stream.
pub struct Scheduler<P: Protocol, H: Handler> {
    inner_state: Inbox<P>,
    futures: FuturesUnordered<Pin<Box<dyn Future<Output = HandlerItem<P, H>> + Send>>>,
    stream: Option<Pin<Box<dyn Stream<Item = HandlerItem<P, H>> + Send>>>,
}

impl<P: Protocol, H: Handler> Scheduler<P, H> {
    /// Schedule a future that resolves to an [`Action<H>`].
    pub fn schedule_future(
        &mut self,
        fut: impl Future<Output = HandlerResult<H>> + Send + 'static,
    ) {
        self.futures.push(Box::pin(
            async move { HandlerItem::HandlerResult(fut.await) },
        ))
    }

    /// Schedule a future that resolves to an [`Action<H>`].
    pub fn schedule_action(&mut self, fut: impl Future<Output = Action<H>> + Send + 'static) {
        self.futures
            .push(Box::pin(async move { HandlerItem::Action(fut.await) }))
    }

    /// Schedule an [`Action<H>`] to be run.
    pub fn schedule_action_now(&mut self, action: Action<H>) {
        self.schedule_action(async move { action })
    }

    /// Schedule a future that resolves to a [`Protocol`].
    pub fn schedule_protocol(&mut self, fut: impl Future<Output = P> + Send + 'static) {
        self.futures
            .push(Box::pin(async move { HandlerItem::Protocol(fut.await) }))
    }

    /// Schedule a [`Protocol`] to be handled.
    pub fn schedule_protocol_now(&mut self, protocol: P) {
        self.futures
            .push(Box::pin(async move { HandlerItem::Protocol(protocol) }))
    }

    /// Schedule a [`Message`] to be handled.
    pub fn schedule_msg_now<M: Message>(&mut self, msg: M) -> M::Returned
    where
        P: FromPayload<M>,
    {
        let (payload, returned) = M::create(msg);
        self.schedule_protocol_now(FromPayload::<M>::from_payload(payload));
        returned
    }

    /// Schedule a [`Stream`] to be run.
    ///
    /// Fails if another stream was already scheduled.
    pub fn schedule_stream<S>(&mut self, stream: S) -> Result<(), S>
    where
        S: Stream<Item = HandlerItem<P, H>> + Send + 'static,
    {
        if self.stream.is_some() {
            Err(stream)
        } else {
            self.stream = Some(Box::pin(stream));
            Ok(())
        }
    }

    /// Remove the [`Stream`] if it was scheduled.
    pub fn remove_stream(
        &mut self,
    ) -> Option<Pin<Box<dyn Stream<Item = HandlerItem<P, H>> + Send>>> {
        self.stream.take()
    }
}

impl<H, P> HandlerState<H> for Scheduler<P, H>
where
    H: Handler,
    P: Protocol + HandledBy<H>,
{
    type Protocol = P;
    type InboxType = Inbox<P>;

    fn from_inbox(inbox: Self::InboxType) -> Self {
        Self {
            inner_state: inbox,
            futures: FuturesUnordered::new(),
            stream: None,
        }
    }

    fn poll_next_item(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<HandlerItem<P, H>> {
        let this = &mut *self;
        #[allow(unused)]
        let mut dead = true;

        if let Poll::Ready(Some(item)) = Pin::new(&mut this.inner_state).poll_next_unpin(cx) {
            match item {
                Ok(protocol) => return Poll::Ready(HandlerItem::Protocol(protocol)),
                Err(_) => return Poll::Ready(HandlerItem::Event(Event::Halted)),
            }
        } else {
            dead = false
        }

        if let Poll::Ready(Some(item)) = this.futures.poll_next_unpin(cx) {
            if !item.is_dead() {
                return Poll::Ready(item);
            }
        } else {
            dead = false
        }

        if let Some(stream) = &mut this.stream {
            if let Poll::Ready(Some(item)) = stream.poll_next_unpin(cx) {
                if !item.is_dead() {
                    return Poll::Ready(item);
                }
            } else {
                dead = false
            }
        }

        if dead {
            Poll::Ready(HandlerItem::Event(Event::Dead))
        } else {
            Poll::Pending
        }
    }
}

impl<P: Protocol, H: Handler> ActorRef for Scheduler<P, H> {
    type ActorType = Inbox<P>;

    fn channel_ref(actor_ref: &Self) -> &Arc<<Self::ActorType as ActorType>::Channel> {
        Inbox::<P>::channel_ref(&actor_ref.inner_state)
    }
}
