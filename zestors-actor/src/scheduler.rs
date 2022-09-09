use crate::*;
use futures::{Future, Stream, StreamExt};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

pub trait Scheduler<P>: Sized + Unpin {
    fn poll_next(pin: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Event<P>>;
}

impl<T, P> Scheduler<P> for T
where
    T: Stream<Item = P> + Unpin,
{
    fn poll_next(mut pin: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Event<P>> {
        pin.poll_next_unpin(cx).map(|protocol| match protocol {
            Some(msg) => Event::Msg(msg),
            None => Event::Disable,
        })
    }
}

pub enum Event<P> {
    /// Disabled the scheduler. This means the scheduler will never be polled again.
    Disable,
    /// No message right now, but there could be new ones in the future.
    /// In the next loop, the scheduler will be polled again.
    Empty,
    /// There is a message that should be handled now.
    Msg(P),
}

pub(super) struct NextEvent<'a, A>(&'a mut A);

impl<'a, A> Unpin for NextEvent<'a, A> {}

impl<'a, A: Actor> Future for NextEvent<'a, A> {
    type Output = Event<A::Protocol>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        <A as Scheduler<A::Protocol>>::poll_next(Pin::new(&mut self.0), cx)
    }
}

impl<'a, A> NextEvent<'a, A> {
    pub fn new(actor: &'a mut A) -> Self {
        Self(actor)
    }
}
