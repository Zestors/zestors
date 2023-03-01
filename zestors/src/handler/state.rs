use crate::all::*;
use futures::Future;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

/// The handler-state generates [`HandlerItem`]s that the [`Handler`] can subsequently handle. It
/// is also passed along to every handler-function as the second argument.
pub trait HandlerState<H: Handler>:
    ActorRef<ActorType = Self::InboxType> + Unpin + Send + 'static
{
    /// The [`Protocol`] of this state.
    type Protocol: Protocol + HandledBy<H>;

    /// The [`InboxType`] of this state.
    type InboxType: InboxType;

    /// Create the state from the [`Self::InboxType`].
    fn from_inbox(inbox: Self::InboxType) -> Self;

    /// Poll the next [`HandlerItem`].
    /// 
    /// This method is very similar to [`futures::Stream::poll_next`], with the main difference that this
    /// should not panic, even after a [`HandlerEvent::Dead`] has been returned.
    fn poll_next_action(self: Pin<&mut Self>, cx: &mut Context) -> Poll<HandlerItem<H>>;
}

/// An item returned from a [`HandlerState`].
pub enum HandlerItem<H: Handler> {
    Protocol(HandlerProtocol<H>),
    Action(Action<H>),
    Halted,
    ClosedAndEmpty,
    Dead,
}

/// An extension to [`HandlerState`].
pub trait HandlerStateExt<H: Handler>: HandlerState<H> {
    /// Get the next [`HandlerItem`] from this handler-state.
    fn next_handler_item(&mut self) -> NextHandlerItem<'_, Self, H> {
        NextHandlerItem(self, PhantomData)
    }
}
impl<T: HandlerState<H>, H: Handler> HandlerStateExt<H> for T {}

/// A future that resolves to the next [`Action<H>`] or [`HandlerEvent`].
pub struct NextHandlerItem<'a, S: HandlerState<H>, H: Handler>(&'a mut S, PhantomData<H>);

impl<'a, S: HandlerState<H>, H: Handler> Unpin for NextHandlerItem<'a, S, H> {}

impl<'a, S: HandlerState<H>, H: Handler> Future for NextHandlerItem<'a, S, H> {
    type Output = HandlerItem<H>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut *self.0).poll_next_action(cx)
    }
}
