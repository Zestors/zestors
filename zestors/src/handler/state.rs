use crate::all::*;
use futures::Future;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

/// The handler-state generates [`HandlerItem`]s that the [`Handler`] `H` can handle.
pub trait HandlerState<H: Handler>: ActorRef<ActorType = Self::InboxType> + Unpin + Send {
    /// The [`Protocol`] of this state.
    type Protocol: Protocol + HandledBy<H>;

    /// The [`InboxType`] of this state.
    type InboxType: InboxType;

    /// Create the state from the [`Self::InboxType`].
    fn from_inbox(inbox: Self::InboxType) -> Self;

    /// Poll the next [`HandlerItem`].
    ///
    /// This method is very similar to [`futures::Stream::poll_next`], with the main difference that this
    /// may continue being polled even if [`Event::Dead`] is returned.
    fn poll_next_item(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<HandlerItem<Self::Protocol, H>>;
}

/// An item returned from a [`HandlerState`].
pub enum HandlerItem<P, S: Handler> {
    Protocol(P),
    Action(Action<S>),
    Event(Event),
    HandlerResult(Result<Flow<S>, S::Exception>),
}

impl<P, H: Handler> HandlerItem<P, H> {
    pub fn is_dead(&self) -> bool {
        if let Self::Event(Event::Dead) = self {
            true
        } else {
            false
        }
    }
}

impl<P, H> HandlerItem<P, H>
where
    H: Handler,
    P: HandledBy<H>,
{
    /// Handle the item with [`Handler`] `H`.
    pub async fn handle_with(self, handler: &mut H, state: &mut H::State) -> HandlerResult<H> {
        match self {
            HandlerItem::Event(event) => handler.handle_event(state, event).await,
            HandlerItem::Action(action) => action.handle_with(handler, state).await,
            HandlerItem::Protocol(protocol) => protocol.handle_with(handler, state).await,
            HandlerItem::HandlerResult(result) => result,
        }
    }
}

/// An extension to [`HandlerState`].
pub trait HandlerStateExt<H: Handler>: HandlerState<H> {
    /// Returns a future that resolves to the next [`HandlerItem<H>`].
    fn next_handler_item(&mut self) -> NextHandlerItem<'_, Self, H>
    where
        Self: Unpin,
    {
        NextHandlerItem(self, PhantomData)
    }
}
impl<T: HandlerState<H>, H: Handler> HandlerStateExt<H> for T {}

/// A future that resolves to a [`HandlerItem`]
pub struct NextHandlerItem<'a, S: HandlerState<H>, H: Handler>(&'a mut S, PhantomData<H>);

impl<'a, S: HandlerState<H>, H: Handler> Unpin for NextHandlerItem<'a, S, H> {}

impl<'a, S: HandlerState<H> + Unpin, H: Handler> Future for NextHandlerItem<'a, S, H> {
    type Output = HandlerItem<S::Protocol, H>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut *self.0).poll_next_item(cx)
    }
}
