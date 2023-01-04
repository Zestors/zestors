use crate::*;
use futures::Future;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

//------------------------------------------------------------------------------------------------
//  Supervisable
//------------------------------------------------------------------------------------------------

/// Lower level trait that defines what it means to be supervisable. For normal users, this does
/// not have to be implemented, but they can instead use ...
pub(crate) trait PollSupervisable: Unpin {
    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()>;
    fn poll_to_restart(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<bool, StartError>>;
    fn poll_restart(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), StartError>>;
}

impl<S> PollSupervisable for &mut S
where
    S: ?Sized + PollSupervisable + Unpin,
{
    fn poll_supervise(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        S::poll_supervise(Pin::new(&mut **self), cx)
    }
    fn poll_to_restart(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<bool, StartError>> {
        S::poll_to_restart(Pin::new(&mut **self), cx)
    }
    fn poll_restart(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), StartError>> {
        S::poll_restart(Pin::new(&mut **self), cx)
    }
}

impl<S> PollSupervisable for Box<S>
where
    S: PollSupervisable + ?Sized,
{
    fn poll_supervise(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        Pin::new(&mut **self).poll_supervise(cx)
    }

    fn poll_to_restart(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<bool, StartError>> {
        Pin::new(&mut **self).poll_to_restart(cx)
    }

    fn poll_restart(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), StartError>> {
        Pin::new(&mut **self).poll_restart(cx)
    }
}

//------------------------------------------------------------------------------------------------
//  Futures
//------------------------------------------------------------------------------------------------

/// Future returned from calling `supervise()` on anything [Supervisable].
pub struct SuperviseFut<'a, S: ?Sized = dyn DynamicallySupervisable>(pub(crate) &'a mut S);
impl<'a, S: ?Sized> Unpin for SuperviseFut<'a, S> {}
impl<'a, S: ?Sized + PollSupervisable> Future for SuperviseFut<'a, S> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        (Pin::new(&mut self.0)).poll_supervise(cx)
    }
}

/// Future returned from calling `to_restart()` on anything [Supervisable].
pub struct ToRestartFut<'a, S: ?Sized = dyn DynamicallySupervisable>(pub(crate) &'a mut S);
impl<'a, S: ?Sized> Unpin for ToRestartFut<'a, S> {}
impl<'a, S: ?Sized + PollSupervisable> Future for ToRestartFut<'a, S> {
    type Output = Result<bool, StartError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        (Pin::new(&mut self.0)).poll_to_restart(cx)
    }
}

/// Future returned from calling `restart()` on anything [Supervisable].
pub struct RestartFut<'a, S: ?Sized = dyn DynamicallySupervisable>(pub(crate) &'a mut S);
impl<'a, S: ?Sized> Unpin for RestartFut<'a, S> {}
impl<'a, S: ?Sized + PollSupervisable> Future for RestartFut<'a, S> {
    type Output = Result<(), StartError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        (Pin::new(&mut self.0)).poll_restart(cx)
    }
}

//------------------------------------------------------------------------------------------------
//  DynamicallySupervisable
//------------------------------------------------------------------------------------------------

pub trait DynamicallySupervisable {
    fn supervise(&mut self) -> SuperviseFut;
    fn to_restart(&mut self) -> ToRestartFut;
    fn restart(&mut self) -> RestartFut;
}

impl<T> DynamicallySupervisable for T
where
    T: PollSupervisable + Send + 'static,
{
    fn supervise(&mut self) -> SuperviseFut {
        SuperviseFut(self)
    }

    fn to_restart(&mut self) -> ToRestartFut {
        ToRestartFut(self)
    }

    fn restart(&mut self) -> RestartFut {
        RestartFut(self)
    }
}
