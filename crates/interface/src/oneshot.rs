use futures::{Future, FutureExt};
use std::{
    fmt::Debug,
    mem::ManuallyDrop,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot;

/// A request that expects a [`Response`] to be sent.
pub struct Request<T>(oneshot::Sender<T>);

impl<T> Request<T> {
    /// Send a message.
    pub fn reply(self, msg: T) -> Result<(), ResolveError<T>> {
        self.into_inner().send(msg).map_err(|msg| ResolveError(msg))
    }

    pub fn no_reply(self) {
        self.into_inner();
    }

    /// Whether the [`Rx`] has closed/dropped the oneshot-channel.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

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

impl<M> Debug for Request<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tx").finish()
    }
}

/// A response that can be awaited to receive the reply.
#[must_use = "Response should be awaited to receive the message"]
pub struct Reply<M>(oneshot::Receiver<M>);

impl<M> Reply<M> {
    /// Attempt to take the message out, if it exists.
    pub fn try_recv(&mut self) -> Result<Option<M>, ReceiptError> {
        match self.0.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(oneshot::error::TryRecvError::Empty) => Ok(None),
            Err(oneshot::error::TryRecvError::Closed) => Err(ReceiptError),
        }
    }

    /// Block the thread while waiting for the message.
    pub fn recv_blocking(self) -> Result<M, ReceiptError> {
        self.0.blocking_recv().map_err(|e| e.into())
    }

    /// Close the oneshot-channel, preventing the [`Tx`] from sending a message.
    pub fn close(&mut self) {
        self.0.close()
    }
}

impl<M> Unpin for Reply<M> {}

impl<M> Future for Reply<M> {
    type Output = Result<M, ReceiptError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll_unpin(cx).map_err(|e| e.into())
    }
}

impl<M> Debug for Reply<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rx").finish()
    }
}

impl<M> Drop for Request<M> {
    fn drop(&mut self) {
        tracing::warn!("Request dropped without being replied to.");
    }
}

/// The actor failed to resolve the receipt correctly.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to receive from Rx because it is closed.")]
pub struct ReceiptError;

impl From<oneshot::error::RecvError> for ReceiptError {
    fn from(_: oneshot::error::RecvError) -> Self {
        Self
    }
}

/// Failed to resolve the [`Receipt`](crate::Receipt), because the
/// receipt has been dropped.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to send to Tx because it is closed.")]
pub struct ResolveError<M>(pub M);
