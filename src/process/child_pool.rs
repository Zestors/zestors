use std::{any::TypeId, time::Duration};

use futures::{Future, Stream, StreamExt};
use tiny_actor::{Channel, ExitError, Inbox, SpawnError, TrySpawnError};
use tokio::task::JoinHandle;

use crate::{
    _gen,
    channel::{ActorType, DynChannel},
    protocol::Protocol,
    AcceptsAll,
};

use crate::*;

/// A `ChildPool` is very similar to a [Child], except that this supervises an actor with multiple
/// processes.
///
/// # Streaming
/// An`Child` can be streamed and will return a stream of `Result<E, ExitError>` when all processes
/// exit.
///
/// #### _For more information, please read the [module](crate) documentation._
pub struct ChildPool<E: Send + 'static, T: ActorType = AcceptsAll![]> {
    inner: tiny_actor::ChildPool<E, T::Channel>,
}

impl<E, T> ChildPool<E, T>
where
    E: Send + 'static,
    T: ActorType,
{
    _gen::channel_methods!(inner);
    _gen::send_methods!(inner);
    _gen::child_methods!(inner);

    pub(crate) fn from_inner(inner: tiny_actor::ChildPool<E, T::Channel>) -> Self {
        Self { inner }
    }

    /// Get the inner [tokio::task::JoinHandle]s.
    ///
    /// Note that this will not run the destructor, and therefore will not halt/abort the actor.
    pub fn into_joinhandles(self) -> Vec<JoinHandle<E>> {
        self.inner.into_joinhandles()
    }

    /// The amount of `tasks` that are running at this moment.
    pub fn task_count(&self) -> usize {
        self.inner.task_count()
    }

    /// The amount of `handles` that this `ChildPool` contains.
    pub fn handle_count(&self) -> usize {
        self.inner.handle_count()
    }

    /// Shutdown the actor.
    ///
    /// This will first halt the actor. If the actor has not exited before the timeout,
    /// it will be aborted instead.
    pub fn shutdown(&mut self, timeout: Duration) -> ShutdownStream<'_, E, T::Channel> {
        ShutdownStream(self.inner.shutdown(timeout))
    }

    /// Attempt to spawn another process onto the actor.
    ///
    /// This can fail if `P` is not the correct [Protocol].
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

impl<E, P> ChildPool<E, P>
where
    E: Send + 'static,
    P: ActorType<Channel = Channel<P>>,
{
    _gen::into_dyn_methods!(inner, ChildPool<E, T>);

    /// Spawn another process onto the actor.
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

impl<E, D> ChildPool<E, D>
where
    E: Send + 'static,
    D: ActorType<Channel = dyn DynChannel>,
{
    _gen::unchecked_send_methods!(inner);
    _gen::transform_methods!(inner, ChildPool<E, T>);
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

//------------------------------------------------------------------------------------------------
//  ShutdownStream
//------------------------------------------------------------------------------------------------

/// Stream returned when shutting down a `ChildPool`.
pub struct ShutdownStream<'a, E: Send + 'static, T: DynChannel + ?Sized>(
    tiny_actor::ShutdownStream<'a, E, T>,
);

impl<'a, E: Send + 'static, T: DynChannel + ?Sized> Unpin for ShutdownStream<'a, E, T> {}

impl<'a, E: Send + 'static, T: DynChannel + ?Sized> Stream for ShutdownStream<'a, E, T> {
    type Item = Result<E, ExitError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.0.poll_next_unpin(cx)
    }
}
