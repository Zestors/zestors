use futures::{Future, FutureExt};
use std::{
    fmt::Debug,
    mem::ManuallyDrop,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot;

/// A request that expects a [`Reply`] to be sent.
pub struct Request<T>(oneshot::Sender<T>);

impl<T> Request<T> {
    /// Sends the reply, resolving the [`Reply`] on the other end.
    pub fn reply(self, msg: T) -> Result<(), ResolveError<T>> {
        self.into_inner().send(msg).map_err(|msg| ResolveError(msg))
    }

    /// Drops the request without sending a reply. Unlike a bare `drop`, this
    /// does not log the "dropped without being replied to" warning, since the
    /// lack of a reply is intentional here.
    pub fn no_reply(self) {
        self.into_inner();
    }

    /// Whether the [`Reply`] has closed/dropped the oneshot-channel.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    /// Creates a new [`Request`]/[`Reply`] pair.
    pub fn new() -> (Request<T>, Reply<T>) {
        let (tx, rx) = oneshot::channel();
        (Request(tx), Reply(rx))
    }

    /// Extracts the inner sender without running [`Drop::drop`].
    fn into_inner(self) -> oneshot::Sender<T> {
        let this = ManuallyDrop::new(self);
        // SAFETY: `this` is never dropped (it's a `ManuallyDrop`) and is not
        // accessed again after this read, so no double-use of the sender occurs.
        unsafe { std::ptr::read(&this.0) }
    }
}

impl<T> Debug for Request<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request").finish()
    }
}

/// A reply that can be awaited to receive the message.
#[must_use = "Reply should be awaited to receive the message"]
pub struct Reply<T>(oneshot::Receiver<T>);

impl<T> Reply<T> {
    /// Returns the message immediately if it has already arrived, without
    /// blocking or yielding. Returns `Ok(None)` if the reply just hasn't
    /// arrived yet, or `Err` if the [`Request`] was dropped without replying.
    pub fn try_wait(&mut self) -> Result<Option<T>, ReceiptError> {
        match self.0.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(oneshot::error::TryRecvError::Empty) => Ok(None),
            Err(oneshot::error::TryRecvError::Closed) => Err(ReceiptError),
        }
    }

    /// Blocks the current thread until the message arrives or the
    /// [`Request`] is dropped without replying. For use outside an async
    /// context — inside one, await the `Reply` itself instead.
    pub fn wait_blocking(self) -> Result<T, ReceiptError> {
        self.0.blocking_recv().map_err(|e| e.into())
    }

    /// Closes the oneshot-channel, preventing the [`Request`] from sending a message.
    pub fn close(&mut self) {
        self.0.close()
    }
}

impl<T> Unpin for Reply<T> {}

impl<T> Future for Reply<T> {
    type Output = Result<T, ReceiptError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll_unpin(cx).map_err(|e| e.into())
    }
}

impl<T> Debug for Reply<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reply").finish()
    }
}

impl<T> Drop for Request<T> {
    fn drop(&mut self) {
        tracing::warn!(
            "Request `{}` dropped without being replied to",
            std::any::type_name::<T>()
        );
    }
}

/// The actor failed to resolve the receipt correctly.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to receive from Reply because it is closed.")]
pub struct ReceiptError;

impl From<oneshot::error::RecvError> for ReceiptError {
    fn from(_: oneshot::error::RecvError) -> Self {
        Self
    }
}

/// Failed to resolve the [`Receipt`](crate::Receipt), because the
/// receipt has been dropped.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to send to Request because it is closed.")]
pub struct ResolveError<T>(pub T);
