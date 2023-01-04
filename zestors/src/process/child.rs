use crate::{
    channel::{ActorType, DynChannel},
    AcceptsAll, _gen,
};

use crate::*;
use futures::{Future, FutureExt};
use std::{any::TypeId, fmt::Debug, time::Duration};
use tiny_actor::{Channel, ExitError};
use tokio::task::JoinHandle;

//------------------------------------------------------------------------------------------------
//  Child
//------------------------------------------------------------------------------------------------

/// # Child<E, T>
/// The first generic `E` indicates what the `task` will exit with, while the second generic
///  `T` indicates what messages can be sent to the actor. For more information about `T`, please
/// refer to the docs on [Address].
///
/// # Awaiting
/// An`Child` can be awaited and will return a `Result<E, ExitError>` once the actor has exited.
///
/// #### _For more information, please read the [module](crate) documentation._
pub struct Child<E: Send + 'static, T: ActorType = AcceptsAll![]> {
    inner: tiny_actor::Child<E, T::Channel>,
}

//------------------------------------------------------------------------------------------------
//  Implementations
//------------------------------------------------------------------------------------------------

impl<E, T> Child<E, T>
where
    E: Send + 'static,
    T: ActorType,
{
    _gen::channel_methods!(inner);
    _gen::send_methods!(inner);
    _gen::child_methods!(inner);

    pub(crate) fn from_inner(inner: tiny_actor::Child<E, T::Channel>) -> Self {
        Self { inner }
    }

    /// Get the inner [tokio::task::JoinHandle].
    ///
    /// Note that this will not run the destructor, and therefore will not halt/abort the actor.
    pub fn into_joinhandle(self) -> JoinHandle<E> {
        self.inner.into_joinhandle()
    }

    /// Convert the `Child` into a `ChildPool`.
    pub fn into_pool(self) -> ChildPool<E, T> {
        ChildPool::from_inner(self.inner.into_pool())
    }

    /// Shutdown the actor.
    ///
    /// This will first halt the actor. If the actor has not exited before the timeout,
    /// it will be aborted instead.
    pub fn shutdown(&mut self, timeout: Duration) -> ShutdownFut<'_, E, T::Channel> {
        ShutdownFut(self.inner.shutdown(timeout))
    }
}

impl<E, P> Child<E, P>
where
    E: Send + 'static,
    P: ActorType<Channel = Channel<P>>,
{
    _gen::into_dyn_methods!(inner, Child<E, T>);
}

impl<E, D> Child<E, D>
where
    E: Send + 'static,
    D: ActorType<Channel = dyn DynChannel>,
{
    _gen::unchecked_send_methods!(inner);
    _gen::transform_methods!(inner, Child<E, T>);
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

//------------------------------------------------------------------------------------------------
//  ShutdownFut
//------------------------------------------------------------------------------------------------

/// Future returned when shutting down a [Child].
pub struct ShutdownFut<'a, E: Send + 'static, T: DynChannel + ?Sized>(
    tiny_actor::ShutdownFut<'a, E, T>,
);

impl<'a, E: Send + 'static, T: DynChannel + ?Sized> Future for ShutdownFut<'a, E, T> {
    type Output = Result<E, ExitError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.0.poll_unpin(cx)
    }
}

impl<'a, E: Send + 'static, T: DynChannel + ?Sized> Unpin for ShutdownFut<'a, E, T> {}
