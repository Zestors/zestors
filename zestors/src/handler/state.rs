use super::action::Action;
use crate::all::*;
use futures::Future;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

pub trait HandlerState<H: Handler>:
    ActorRef<ActorType = Self::InboxType> + Unpin + Send + 'static
{
    /// The protocol that this state returns.
    type Protocol: Protocol + HandledBy<H>;

    /// The inbox that this state uses under the hood.
    type InboxType: ActorInbox;

    /// Create the state from the inbox.
    fn from_inbox(inbox: Self::InboxType) -> Self;

    /// Poll the next [`Action<H>`] or [`HandlerEvent`].
    /// This should be repeatedly pollable, even after a [`HandlerEvent::Dead`] has been
    /// returned.
    fn poll_next_action(self: Pin<&mut Self>, cx: &mut Context) -> Poll<HandlerItem<H>>;
}

pub enum HandlerItem<H: Handler> {
    Protocol(HandlerProtocol<H>),
    Action(Action<H>),
    Event(Event),
}

impl<H: Handler> HandlerItem<H> {
    pub async fn handle_with(
        self,
        handler: &mut H,
        state: &mut H::State,
    ) -> Result<Flow, H::Exception> {
        match self {
            HandlerItem::Protocol(protocol) => protocol.handle_with(handler, state).await,
            HandlerItem::Action(action) => action.handle(handler, state).await,
            HandlerItem::Event(event) => handler.handle_event(state, event).await,
        }
    }
}

/// A special event that should be handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Event {
    /// This process has been halted.
    Halt,
    /// This actor's inbox is closed and empty.
    ClosedAndEmpty,
    /// This actor's inbox is closed and empty, and no futures have been scheduled either.
    /// The actor may be revived by scheduling a new future.
    Dead,
}

pub trait HandlerStateExt<H: Handler>: HandlerState<H> {
    /// Get the next [`Action<H>`] or [`HandlerEvent`] from this handler-state.
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
