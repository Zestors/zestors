#[allow(unused)]
use crate::all::*;
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

pub struct BoxSupervisee<Ref = ()>(Box<dyn DynSupervisee<Ref = Ref>>);

impl<Ref: Send + 'static> BoxSupervisee<Ref> {
    pub fn new<S: Supervisee<Ref = Ref>>(supervisee: S) -> Self {
        Self(Box::new(supervisee))
    }
}

impl<Ref: Send + 'static> Supervisee for BoxSupervisee<Ref> {
    type Ref = Ref;
    type Running = RunningBoxSupervisee<Ref>;
    type Future = BoxFuture<'static, Result<(Self::Running, Self::Ref), Self>>;

    fn start_supervised(self) -> Self::Future {
        self.0.start_supervised()
    }
}

impl<Ref> Unpin for BoxSupervisee<Ref> {}

pub struct RunningBoxSupervisee<Ref = ()>(Pin<Box<dyn DynRunningSupervisee<Ref = Ref>>>);

impl<Ref: Send + 'static> RunningBoxSupervisee<Ref> {
    pub fn new<S: RunningSupervisee + 'static>(supervisee: S) -> Self
    where
        S::Supervisee: Supervisee<Ref = Ref>,
    {
        Self(Box::pin(supervisee))
    }
}

impl<Ref: Send + 'static> RunningSupervisee for RunningBoxSupervisee<Ref> {
    type Supervisee = BoxSupervisee<Ref>;

    fn poll_supervise(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<Self::Supervisee>> {
        self.0.as_mut().poll_supervise(cx)
    }

    fn shutdown_time(self: Pin<&Self>) -> Duration {
        self.0.as_ref().shutdown_time()
    }

    fn halt(mut self: Pin<&mut Self>) {
        self.0.as_mut().halt()
    }

    fn abort(mut self: Pin<&mut Self>) {
        self.0.as_mut().abort()
    }
}

impl<Ref> Unpin for RunningBoxSupervisee<Ref> {}

//------------------------------------------------------------------------------------------------
//  Dynamic traits
//------------------------------------------------------------------------------------------------

#[async_trait]
trait DynSupervisee: Send + 'static {
    type Ref: Send + 'static;

    async fn start_supervised(
        self: Box<Self>,
    ) -> Result<(RunningBoxSupervisee<Self::Ref>, Self::Ref), BoxSupervisee<Self::Ref>>;
}

#[async_trait]
impl<T, Ref: Send + 'static> DynSupervisee for T
where
    T: Supervisee<Ref = Ref>,
{
    type Ref = Ref;

    async fn start_supervised(
        self: Box<T>,
    ) -> Result<(RunningBoxSupervisee<Ref>, Ref), BoxSupervisee<Ref>> {
        match Supervisee::start_supervised(*self).await {
            Ok((running, reference)) => Ok((RunningBoxSupervisee::new(running), reference)),
            Err(this) => Err(BoxSupervisee::new(this)),
        }
    }
}

#[async_trait]
pub trait DynRunningSupervisee: Send {
    type Ref: Send + 'static;

    fn poll_supervise(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<BoxSupervisee<Self::Ref>>>;
    fn shutdown_time(self: Pin<&Self>) -> Duration;
    fn halt(self: Pin<&mut Self>);
    fn abort(self: Pin<&mut Self>);
}

impl<T> DynRunningSupervisee for T
where
    T: RunningSupervisee,
{
    type Ref = <T::Supervisee as Supervisee>::Ref;

    fn poll_supervise(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<BoxSupervisee<Self::Ref>>> {
        RunningSupervisee::poll_supervise(self, cx).map(|res| match res {
            Some(supervisee) => Some(BoxSupervisee::new(supervisee)),
            None => None,
        })
    }

    fn shutdown_time(self: Pin<&Self>) -> Duration {
        RunningSupervisee::shutdown_time(self)
    }

    fn halt(self: Pin<&mut Self>) {
        RunningSupervisee::halt(self)
    }

    fn abort(self: Pin<&mut Self>) {
        RunningSupervisee::abort(self)
    }
}
