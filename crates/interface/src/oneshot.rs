use futures::{Future, FutureExt};
use std::{
    fmt::Debug,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot;

pub struct Request<T>(oneshot::Sender<T>);

impl<T> Request<T> {
    /// Send a message.
    pub fn send(self, msg: T) -> Result<(), ReplyError<T>> {
        self.0.send(msg).map_err(|msg| ReplyError(msg))
    }

    /// Whether the [`Rx`] has closed/dropped the oneshot-channel.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    /// Wait for the [`Rx`] to close/drop the oneshot-channel.
    pub async fn closed(&mut self) {
        self.0.closed().await
    }

    pub fn new() -> (Request<T>, Response<T>) {
        let (tx, rx) = oneshot::channel();
        (Request(tx), Response(rx))
    }
}

impl<M> Debug for Request<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tx").finish()
    }
}

#[must_use = "Response should be awaited to receive the message"]
pub struct Response<M>(oneshot::Receiver<M>);

impl<M> Response<M> {
    /// Attempt to take the message out, if it exists.
    pub fn try_recv(&mut self) -> Result<Option<M>, ResponseError> {
        match self.0.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(oneshot::error::TryRecvError::Empty) => Ok(None),
            Err(oneshot::error::TryRecvError::Closed) => Err(ResponseError),
        }
    }

    /// Block the thread while waiting for the message.
    pub fn recv_blocking(self) -> Result<M, ResponseError> {
        self.0.blocking_recv().map_err(|e| e.into())
    }

    /// Close the oneshot-channel, preventing the [`Tx`] from sending a message.
    pub fn close(&mut self) {
        self.0.close()
    }
}

impl<M> Unpin for Response<M> {}

impl<M> Future for Response<M> {
    type Output = Result<M, ResponseError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll_unpin(cx).map_err(|e| e.into())
    }
}

impl<M> Debug for Response<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rx").finish()
    }
}

//------------------------------------------------------------------------------------------------
//  Errors
//------------------------------------------------------------------------------------------------

/// Error returned when receiving a message using an [`Rx`].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to receive from Rx because it is closed.")]
pub struct ResponseError;

impl From<oneshot::error::RecvError> for ResponseError {
    fn from(_: oneshot::error::RecvError) -> Self {
        Self
    }
}

/// Error returned when sending a message using a [`Tx`].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to send to Tx because it is closed.")]
pub struct ReplyError<M>(pub M);
