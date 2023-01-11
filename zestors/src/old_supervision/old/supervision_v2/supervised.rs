use super::*;
use crate::*;
use async_trait::async_trait;
use futures::{Future, FutureExt};
use std::{
    error::Error,
    mem,
    pin::Pin,
    task::{Context, Poll},
};
use tiny_actor::Link;

//------------------------------------------------------------------------------------------------
//  SupervisedChild
//------------------------------------------------------------------------------------------------

/// A `SupervisedChild` defines exactly how a child may be supervised. This can be added to a
/// supervisor in order to automatically supervise and restart the process.
///
/// A new `SupervisedChild` may be created using the [SupervisedChild::new] function for complete
/// control over its restart behaviour. Normally, a `SupervisedChild` is created from a [ChildSpec]
/// using [ChildSpec::spawn].
///
/// A `SupervisedChild<P, E, Fun, Fut>` can be converted into a [DynamicSupervisedChild] using
/// [SupervisedChild::into_dyn]. This removes its type-information, but it can still be supervised.
pub struct SupervisedChild<P, E, Fun, Fut>
where
    E: Send + 'static,
    P: Protocol,
{
    child: Child<E, P>,
    start_fn: Fun,
    restart_fut: Option<Pin<Box<Fut>>>,
}

impl<P, E, Fun, Fut> SupervisedChild<P, E, Fun, Fut>
where
    P: Protocol + Send,
    E: Send + 'static,
    Fun: FnMut(StartOption<E>, Link) -> Fut + Send + 'static,
    Fut: Future<Output = RestartResult<Option<Child<E, P>>>> + Send + 'static,
{
    pub fn new(child: Child<E, P>, start_fn: Fun) -> Self {
        Self {
            child,
            start_fn,
            restart_fut: None,
        }
    }

    pub fn into_dyn(self) -> DynamicSupervisedChild {
        DynamicSupervisedChild(Box::new(self))
    }

    pub fn supervise(&mut self) -> SuperviseFut<'_, P, E, Fun, Fut> {
        SuperviseFut(self)
    }

    pub async fn restart(&mut self) -> Result<Supervision, StartError> {
        let restart_fut = self.restart_fut.as_mut().expect("Should have exited");

        match restart_fut.await {
            Ok(Some(mut child)) => {
                mem::swap(&mut self.child, &mut child);
                Ok(Supervision::Restarted)
            }
            Ok(None) => Ok(Supervision::Finished),
            Err(e) => Err(e),
        }
    }
}

pub struct SuperviseFut<'a, P, E, Fun, Fut>(&'a mut SupervisedChild<P, E, Fun, Fut>)
where
    P: Protocol,
    E: Send + 'static;

impl<'a, P, E, Fun, Fut> Unpin for SuperviseFut<'a, P, E, Fun, Fut>
where
    P: Protocol,
    E: Send + 'static,
{
}

impl<'a, P, E, Fun, Fut> Future for SuperviseFut<'a, P, E, Fun, Fut>
where
    P: Protocol + Send,
    E: Send + 'static,
    Fun: FnMut(StartOption<E>, Link) -> Fut + Send + 'static,
    Fut: Future<Output = RestartResult<Option<Child<E, P>>>> + Send + 'static,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Poll::Ready(exit) = self.0.child.poll_unpin(cx) {
            let link = self.0.child.link().clone();
            self.0.restart_fut = Some(Box::pin((self.0.start_fn)(
                StartOption::Restart(exit),
                link,
            )));
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

//------------------------------------------------------------------------------------------------
//  DynamicSupervisedChild
//------------------------------------------------------------------------------------------------

pub struct DynamicSupervisedChild(Box<dyn Supervisable + Send>);

impl DynamicSupervisedChild {
    pub fn supervise<'a>(&'a mut self) -> impl Future<Output = ()> + Send + Unpin + 'a {
        self.0.supervise()
    }

    pub fn restart<'a>(
        &'a mut self,
    ) -> impl Future<Output = Result<Supervision, StartError>> + Send + Unpin + 'a {
        self.0.restart()
    }

    pub(super) fn new<S: Supervisable + Send + 'static>(inner: S) -> Self {
        Self(Box::new(inner))
    }
}

//------------------------------------------------------------------------------------------------
//  Supervisable
//------------------------------------------------------------------------------------------------

#[async_trait]
pub(super) trait Supervisable {
    async fn supervise(&mut self);
    async fn restart(&mut self) -> Result<Supervision, StartError>;
}

#[async_trait]
impl<P, E, Fun, Fut> Supervisable for SupervisedChild<P, E, Fun, Fut>
where
    P: Protocol + Send,
    E: Send + 'static,
    Fun: FnMut(StartOption<E>, Link) -> Fut + Send + 'static,
    Fut: Future<Output = RestartResult<Option<Child<E, P>>>> + Send + 'static,
{
    async fn supervise(&mut self) {
        self.supervise().await
    }

    async fn restart(&mut self) -> Result<Supervision, StartError> {
        self.restart().await
    }
}

//------------------------------------------------------------------------------------------------
//  RestartError
//------------------------------------------------------------------------------------------------

/// An error returned if starting of a process has failed.
#[derive(Debug)]
pub struct StartError(pub Box<dyn Error + Send>);

impl StartError {
    pub fn new<E: Error + Send + 'static>(error: E) -> Self {
        Self(Box::new(error))
    }
}

impl<E: Error + Send + 'static> From<E> for StartError {
    fn from(value: E) -> Self {
        Self::new(value)
    }
}

//------------------------------------------------------------------------------------------------
//  Supervision
//------------------------------------------------------------------------------------------------

pub enum Supervision {
    Restarted,
    Finished,
}
