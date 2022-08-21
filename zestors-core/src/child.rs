use std::{any::TypeId, fmt::Debug, time::Duration};

use futures::{Future, FutureExt};
use tokio::task::JoinHandle;

use crate::*;

//------------------------------------------------------------------------------------------------
//  Child
//------------------------------------------------------------------------------------------------

pub struct Child<E: Send + 'static, T: ActorType = Accepts![]> {
    inner: tiny_actor::Child<E, <T::Type as ChannelType>::Channel>,
}

//------------------------------------------------------------------------------------------------
//  Any child
//------------------------------------------------------------------------------------------------

impl<E, T> Child<E, T>
where
    E: Send + 'static,
    T: ActorType,
{
    gen::channel_methods!(inner);
    gen::send_methods!(inner);
    gen::child_methods!(inner);

    pub(crate) fn from_inner(
        inner: tiny_actor::Child<E, <T::Type as ChannelType>::Channel>,
    ) -> Self {
        Self { inner }
    }

    pub fn into_joinhandle(self) -> JoinHandle<E> {
        self.inner.into_joinhandle()
    }

    pub fn into_pool(self) -> ChildPool<E, T> {
        ChildPool::from_inner(self.inner.into_pool())
    }

    pub fn shutdown(
        &mut self,
        timeout: Duration,
    ) -> Shutdown<'_, E, <T::Type as ChannelType>::Channel> {
        self.inner.shutdown(timeout)
    }


}

//-------------------------------------------------
//  Static child
//-------------------------------------------------

impl<E, P> Child<E, P>
where
    E: Send + 'static,
    P: ActorType<Type = Static<P>>,
{
    gen::into_dyn_methods!(inner, Child<E, T>);
}

//-------------------------------------------------
//  Dynamic child
//-------------------------------------------------

impl<E, D> Child<E, D>
where
    E: Send + 'static,
    D: ActorType<Type = Dynamic>,
{
    gen::unchecked_send_methods!(inner);
    gen::transform_methods!(inner, Child<E, T>);
}

//-------------------------------------------------
//  Trait implementations
//-------------------------------------------------

impl<E, T> std::fmt::Debug for Child<E, T>
where
    E: Send + 'static + Debug,
    T: ActorType,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Child").field(&self.inner).finish()
    }
}

impl<E, T> Unpin for Child<E, T>
where
    E: Send + 'static,
    T: ActorType,
{
}

impl<E, T> Future for Child<E, T>
where
    E: Send + 'static,
    T: ActorType,
{
    type Output = Result<E, ExitError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.inner.poll_unpin(cx)
    }
}
