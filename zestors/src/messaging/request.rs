use futures::{Future, FutureExt};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot;
use crate::all::*;

/// Create a new request, consisting of a [`Tx<T>`] and an [`Rx<T>`].
/// The `Tx` (_transmitter_) can be used to send a single message `T` to the `Rx` (_receiver_).
///
/// This is just a wrapper around a [`tokio::sync::oneshot`] channel.
pub fn new_request<T>() -> (Tx<T>, Rx<T>) {
    let (tx, rx) = oneshot::channel();
    (Tx(tx), Rx(rx))
}

//------------------------------------------------------------------------------------------------
//  Tx
//------------------------------------------------------------------------------------------------

/// The transmitter part of a request, created with [`new_request`].
/// 
/// This implements [`MessageDerive<M>`] to be used with the [`Message!`] derive macro.
#[derive(Debug)]
pub struct Tx<M>(pub(super) oneshot::Sender<M>);

impl<M> Tx<M> {
    /// Send a message.
    pub fn send(self, msg: M) -> Result<(), TxError<M>> {
        self.0.send(msg).map_err(|msg| TxError(msg))
    }

    /// Whether the [`Rx`] has closed/dropped the oneshot-channel.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    /// Wait for the [`Rx`] to close/drop the oneshot-channel.
    pub async fn closed(&mut self) {
        self.0.closed().await
    }
}

impl<M, R> MessageDerive<M> for Tx<R> {
    type Payload = (M, Rx<R>);
    type Returned = Tx<R>;

    fn create(msg: M) -> ((M, Rx<R>), Tx<R>) {
        let (tx, rx) = new_request();
        ((msg, rx), tx)
    }

    fn cancel(sent: (M, Rx<R>), _returned: Tx<R>) -> M {
        sent.0
    }
}

//------------------------------------------------------------------------------------------------
//  Rx
//------------------------------------------------------------------------------------------------

/// The receiver part of a request, created with [`new_request`].
/// 
/// This implements [`MessageDerive<M>`] to be used with the [`Message!`] derive macro.
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

    /// Close the oneshot-channel, preventing the [`Tx`] from sending a message.
    pub fn close(&mut self) {
        self.0.close()
    }
}

impl<M, R> MessageDerive<M> for Rx<R> {
    type Payload = (M, Tx<R>);
    type Returned = Rx<R>;

    fn create(msg: M) -> ((M, Tx<R>), Rx<R>) {
        let (tx, rx) = new_request();
        ((msg, tx), rx)
    }

    fn cancel(sent: (M, Tx<R>), _returned: Rx<R>) -> M {
        sent.0
    }
}

impl<M> Unpin for Rx<M> {}

impl<M> Future for Rx<M> {
    type Output = Result<M, RxError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll_unpin(cx).map_err(|e| e.into())
    }
}

//------------------------------------------------------------------------------------------------
//  Errors
//------------------------------------------------------------------------------------------------

/// Error returned when receiving a message using an [`Rx`].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to receive from Rx because it is closed.")]
pub struct RxError;

impl From<oneshot::error::RecvError> for RxError {
    fn from(_: oneshot::error::RecvError) -> Self {
        Self
    }
}

/// Error returned when trying to receive a message using an [`Rx`].
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

/// Error returned when sending a message using a [`Tx`].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to send to Tx because it is closed.")]
pub struct TxError<M>(pub M);
