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
    type Protocol: Protocol + HandledBy<H>;
    type InboxType: InboxType;

    fn from_inbox(inbox: Self::InboxType) -> Self;

    fn poll_next_action(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<Action<H>, HandlerEvent>>;
}

/// A special event for a given actor.
/// All events are unique and will only happen at most once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HandlerEvent {
    /// This process has been halted.
    Halt,
    /// This actor's closed inbox is also empty now.
    ClosedAndEmpty,
    /// This actor's inbox is closed and empty, and no futures have been scheduled either.
    /// The actor may be revived by scheduling a new future.
    Dead,
}

pub trait HandlerStateExt<H: Handler>: HandlerState<H> {
    fn next_action(&mut self) -> NextHandlerAction<'_, Self, H> {
        NextHandlerAction(self, PhantomData)
    }
}
impl<T: HandlerState<H>, H: Handler> HandlerStateExt<H> for T {}

pub struct NextHandlerAction<'a, S: HandlerState<H>, H: Handler>(&'a mut S, PhantomData<H>);

impl<'a, S: HandlerState<H>, H: Handler> Unpin for NextHandlerAction<'a, S, H> {}

impl<'a, S: HandlerState<H>, H: Handler> Future for NextHandlerAction<'a, S, H> {
    type Output = Result<Action<H>, HandlerEvent>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut *self.0).poll_next_action(cx)
    }
}
