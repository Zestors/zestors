use crate::*;
use futures::FutureExt as _;
use std::{fmt::Debug, pin::Pin, task::Poll, time::Duration};

/// A unique handle to a child process spawned on a [`Channel`]. By default,
/// dropping a `Child` will abort the child process. To prevent this, call [`Child::detach`].
///
/// A `Child` is made up of 2 main components:
/// - The [`tokio::task::JoinHandle`] of the child process, which can be used to await the
///   exit of the child process and retrieve its result.
/// - The [`StrongAddress`] of the child process, which can be used to interact with the
/// child process.
pub struct Child<E = (), C: Context = Dyn<()>> {
    join: Option<tokio::task::JoinHandle<Result<E, Report>>>,
    address: StrongAddress<C>,
    attached: bool,
}

impl<E, C: Context> Child<E, C> {
    pub(crate) fn new(
        join: tokio::task::JoinHandle<Result<E, Report>>,
        address: StrongAddress<C>,
    ) -> Self {
        Self {
            join: Some(join),
            address,
            attached: true,
        }
    }

    /// Aborts the child process by canceling the future at the first `.await` point.
    pub fn abort(&self) {
        self.handle().abort();
    }

    pub fn into_handle(mut self) -> tokio::task::JoinHandle<Result<E, Report>> {
        self.join.take().unwrap()
    }

    pub fn handle(&self) -> &tokio::task::JoinHandle<Result<E, Report>> {
        self.join.as_ref().unwrap()
    }

    pub fn into_parts(mut self) -> (tokio::task::JoinHandle<Result<E, Report>>, StrongAddress<C>) {
        (self.join.take().unwrap(), self.address.clone())
    }

    /// Returns the [`Child`] as an `attached` child, which will be aborted when dropped.
    pub fn attached(mut self) -> Self {
        self.attached = true;
        self
    }

    /// Returns the [`Child`] as a `detached` child, which will not be aborted when dropped.
    pub fn detached(mut self) -> Self {
        self.attached = false;
        self
    }

    /// Attaches the [`Child`] to the parent, which will be aborted when dropped.``
    pub fn attach(&mut self) {
        self.attached = true;
    }

    /// Detaches the [`Child`] from the parent, which will not be aborted when dropped.
    pub fn detach(&mut self) {
        self.attached = false;
    }

    /// Whether the [`Child`] is attached to the parent, which will be aborted when dropped.
    pub fn is_attached(&self) -> bool {
        self.attached
    }

    /// Returns the [`StrongAddress`] of the child process.
    pub fn strong_address(&self) -> &StrongAddress<C> {
        &self.address
    }

    /// Signals a shutdown to the child process and waits for it to exit within the given timeout.
    /// If the child process does not exit within the timeout, it is aborted.
    pub async fn shutdown_abort(mut self, timeout: Duration) -> Result<E, ShutdownAbortError> {
        self.address.signal_shutdown();

        match tokio::time::timeout(timeout, &mut self).await {
            Ok(Ok(e)) => Ok(e),
            Ok(Err(err)) => Err(err.into_join_abort(false, timeout)),
            Err(_elapsed) => {
                tracing::warn!("Child did not exit within timeout. Aborting child.");

                self.abort();
                self.await.map_err(|err| err.into_join_abort(true, timeout))
            }
        }
    }

    pub fn into_shutdown(self, duration: Duration) -> ExitingChild<E, C> {
        ExitingChild::new(self, duration)
    }
}

impl<T, R: Context> ActorOps for Child<T, R> {
    type Ctx = R;

    fn handle(&self) -> &Channel<Self::Ctx> {
        self.address.handle()
    }
}

impl<T, R: Context> IntoDyn for Child<T, R> {
    type Ref<S: Context> = Child<T, S>;

    fn into_context_unchecked<S>(self) -> Self::Ref<S>
    where
        S: Context,
    {
        let (handle, address) = self.into_parts();
        Child::new(handle, address.into_context_unchecked())
    }
}

impl<T, R: Context> Future for Child<T, R> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        // 1. Defend against polling after completion
        let Some(join) = self.join.as_mut() else {
            panic!("Child polled after completion");
        };

        // 2. Poll the join handle first
        if let Poll::Ready(res) = join.poll_unpin(cx) {
            self.join.take();
            return Poll::Ready(match res {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(err)) => Err(JoinError::UnhandledError(err)),
                Err(join_err) => Err(join_err.into()),
            });
        }

        Poll::Pending
    }
}

impl<T, R: Context> Drop for Child<T, R> {
    fn drop(&mut self) {
        if self.attached && self.join.is_some() {
            self.abort();
        }
    }
}

impl<T, R: Context> Debug for Child<T, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Child")
            .field("handle", &std::any::type_name::<T>())
            .field("attached", &self.attached)
            .field("address", &self.address)
            .finish()
    }
}

pub struct ExitingChild<E = (), C: Context = Dyn<()>> {
    child: Child<E, C>,
    abort_after: Pin<Box<tokio::time::Sleep>>,
}

impl<E, C: Context> ExitingChild<E, C> {
    pub fn new(child: Child<E, C>, duration: Duration) -> Self {
        child.signal_shutdown();

        Self {
            child,
            abort_after: Box::pin(tokio::time::sleep(duration)),
        }
    }

    pub fn into_inner(self) -> Child<E, C> {
        self.child
    }

    pub fn child(&self) -> &Child<E, C> {
        &self.child
    }

    pub fn child_mut(&mut self) -> &mut Child<E, C> {
        &mut self.child
    }
}

impl<E, C: Context> Future for ExitingChild<E, C> {
    type Output = Result<E, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        if let Poll::Ready(res) = self.child.poll_unpin(cx) {
            return Poll::Ready(res);
        }

        if self.abort_after.poll_unpin(cx).is_ready() {
            self.child.abort();

            if let Poll::Ready(res) = self.child.poll_unpin(cx) {
                return Poll::Ready(res);
            }
        }

        Poll::Pending
    }
}

impl<E, C: Context> Debug for ExitingChild<E, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExitingChild")
            .field("child", &self.child)
            .field("abort_after", &self.abort_after)
            .finish()
    }
}
