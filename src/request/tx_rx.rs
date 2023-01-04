use futures::{Future, FutureExt};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot;

//------------------------------------------------------------------------------------------------
//  Tx
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Tx<M>(pub(super) oneshot::Sender<M>);

impl<M> Tx<M> {
    /// Send a message.
    pub fn send(self, msg: M) -> Result<(), TxError<M>> {
        self.0.send(msg).map_err(|msg| TxError(msg))
    }

    /// Whether the [Rx] has closed or dropped the oneshot-channel.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    /// Wait for the [Rx] to close or drop the oneshot-channel.
    pub async fn closed(&mut self) {
        self.0.closed().await
    }
}

/// Error returned when sending a message using a [Tx].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to send to Tx because it is closed.")]
pub struct TxError<M>(pub M);

//------------------------------------------------------------------------------------------------
//  Rx
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Rx<M>(pub(super) oneshot::Receiver<M>);

impl<M> Rx<M> {
    /// Attempt to take the message out, if it exists.
    pub fn try_recv(&mut self) -> Result<M, TryRxError> {
        self.0.try_recv().map_err(|e| e.into())
    }

    /// Block the thread while waiting for the message.
    pub fn recv_blocking(self) -> Result<M, RxError> {
        self.0.blocking_recv().map_err(|e| e.into())
    }

    /// Close the oneshot-channel, preventing the [Tx] from sending a message.
    pub fn close(&mut self) {
        self.0.close()
    }
}

impl<M> Unpin for Rx<M> {}

impl<M> Future for Rx<M> {
    type Output = Result<M, RxError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll_unpin(cx).map_err(|e| e.into())
    }
}

/// Error returned when receiving a message using an [Rx].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to receive from Rx because it is closed.")]
pub struct RxError;

impl From<oneshot::error::RecvError> for RxError {
    fn from(_: oneshot::error::RecvError) -> Self {
        Self
    }
}

/// Error returned when trying to receive a message using an [Rx](crate::request::Rx).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
pub enum TryRxError {
    #[error("Closed")]
    Closed,
    #[error("Empty")]
    Empty,
}

impl From<oneshot::error::TryRecvError> for TryRxError {
    fn from(e: oneshot::error::TryRecvError) -> Self {
        match e {
            oneshot::error::TryRecvError::Empty => Self::Empty,
            oneshot::error::TryRecvError::Closed => Self::Closed,
        }
    }
}
