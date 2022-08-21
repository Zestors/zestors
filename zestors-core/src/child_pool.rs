use std::{any::TypeId, time::Duration};

use futures::{Future, Stream, StreamExt};
use tokio::task::JoinHandle;

use crate::*;

//------------------------------------------------------------------------------------------------
//  ChildPool
//------------------------------------------------------------------------------------------------

pub struct ChildPool<E: Send + 'static, T: ActorType = Accepts![]> {
    inner: tiny_actor::ChildPool<E, <T::Type as ChannelType>::Channel>,
}

//------------------------------------------------------------------------------------------------
//  Any child-pool
//------------------------------------------------------------------------------------------------

impl<E, T> ChildPool<E, T>
where
    E: Send + 'static,
    T: ActorType,
{
    gen::channel_methods!(inner);
    gen::send_methods!(inner);
    gen::child_methods!(inner);

    pub(crate) fn from_inner(
        inner: tiny_actor::ChildPool<E, <T::Type as ChannelType>::Channel>,
    ) -> Self {
        Self { inner }
    }

    pub fn into_joinhandles(self) -> Vec<JoinHandle<E>> {
        self.inner.into_joinhandles()
    }

    pub fn task_count(&self) -> usize {
        self.inner.task_count()
    }

    pub fn handle_count(&self) -> usize {
        self.inner.handle_count()
    }

    pub fn shutdown(
        &mut self,
        timeout: Duration,
    ) -> ShutdownPool<'_, E, <T::Type as ChannelType>::Channel> {
        self.inner.shutdown(timeout)
    }

    pub fn try_spawn<P, Fun, Fut>(&mut self, fun: Fun) -> Result<(), TrySpawnError<Fun>>
    where
        Fun: FnOnce(Inbox<P>) -> Fut + Send + 'static,
        Fut: Future<Output = E> + Send + 'static,
        P: Protocol,
        E: Send + 'static,
    {
        self.inner.try_spawn(fun)
    }
}

//-------------------------------------------------
//  Static child-pool
//-------------------------------------------------

impl<E, P> ChildPool<E, P>
where
    E: Send + 'static,
    P: ActorType<Type = Static<P>>,
{
    gen::into_dyn_methods!(inner, ChildPool<E, T>);

    pub fn spawn<Fun, Fut>(&mut self, fun: Fun) -> Result<(), SpawnError<Fun>>
    where
        Fun: FnOnce(Inbox<P>) -> Fut + Send + 'static,
        Fut: Future<Output = E> + Send + 'static,
        E: Send + 'static,
        P: Protocol,
    {
        self.inner.spawn(fun)
    }
}

//-------------------------------------------------
//  Dynamic child-pool
//-------------------------------------------------

impl<E, D> ChildPool<E, D>
where
    E: Send + 'static,
    D: ActorType<Type = Dynamic>,
{
    gen::unchecked_send_methods!(inner);
    gen::transform_methods!(inner, ChildPool<E, T>);
}

//-------------------------------------------------
//  Trait implementations
//-------------------------------------------------

impl<E, T> std::fmt::Debug for ChildPool<E, T>
where
    E: Send + 'static + std::fmt::Debug,
    T: ActorType,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Child").field(&self.inner).finish()
    }
}

impl<E, T> Unpin for ChildPool<E, T>
where
    E: Send + 'static,
    T: ActorType,
{
}

impl<E, T> Stream for ChildPool<E, T>
where
    E: Send + 'static,
    T: ActorType,
{
    type Item = Result<E, ExitError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx)
    }
}
