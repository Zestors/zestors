#![doc = include_str!("../../docs/request.md")]

use futures::{Future, FutureExt};
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot;
use zestors_core::{
    error::{SendError, TrySendError},
    messaging::{Message, MessageType},
    process::SendFut,
};

//------------------------------------------------------------------------------------------------
//  Request
//------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct Request<T>(PhantomData<T>);

impl<T> Request<T> {
    pub fn new() -> (Tx<T>, Rx<T>) {
        let (tx, rx) = oneshot::channel();
        (Tx(tx), Rx(rx))
    }
}

impl<M, R> MessageType<M> for Request<R> {
    type Sends = (M, Tx<R>);
    type Returns = Rx<R>;

    fn new_pair(msg: M) -> ((M, Tx<R>), Rx<R>) {
        let (tx, rx) = Request::new();
        ((msg, tx), rx)
    }

    fn into_msg(sends: (M, Tx<R>), _returns: Rx<R>) -> M {
        sends.0
    }
}

//------------------------------------------------------------------------------------------------
//  Tx
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Tx<M>(oneshot::Sender<M>);

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

//------------------------------------------------------------------------------------------------
//  Rx
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Rx<M>(oneshot::Receiver<M>);

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
        self.0.poll_unpin(cx).map_err(|_| RxError)
    }
}

//------------------------------------------------------------------------------------------------
//  RxError
//------------------------------------------------------------------------------------------------

/// Error returned when receiving a message using an [Rx].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to receive from Rx because it is closed.")]
pub struct RxError;

impl From<oneshot::error::RecvError> for RxError {
    fn from(_: oneshot::error::RecvError) -> Self {
        Self
    }
}

//------------------------------------------------------------------------------------------------
//  TryRxError
//------------------------------------------------------------------------------------------------

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

//------------------------------------------------------------------------------------------------
//  TxError
//------------------------------------------------------------------------------------------------

/// Error returned when sending a message using a [Tx].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to send to Tx because it is closed.")]
pub struct TxError<M>(pub M);

//------------------------------------------------------------------------------------------------
//  IntoRecv
//------------------------------------------------------------------------------------------------

/// Error returned when using the [IntoRecv] trait.
///
/// This error combines failures in sending and receiving.
#[derive(Debug)]
pub enum SendRecvError<M> {
    NoReply,
    Closed(M),
}

/// Error returned when using the [IntoRecv] trait.
///
/// This error combines failures in sending and receiving.
#[derive(Debug)]
pub enum TrySendRecvError<M> {
    NoReply,
    Closed(M),
    Full(M),
}

/// Trait that makes it easier to await a reply from a [Request].
pub trait IntoRecv {
    type Recv;

    /// Wait for the reply of a request.
    fn into_recv(self) -> Self::Recv;
}

impl<M, R> IntoRecv for Result<Rx<R>, SendError<M>>
where
    M: Send + 'static,
    R: Send + 'static,
{
    type Recv = Pin<Box<dyn Future<Output = Result<R, SendRecvError<M>>> + Send + 'static>>;

    fn into_recv(self) -> Self::Recv {
        Box::pin(async move {
            match self {
                Ok(rx) => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(_) => Err(SendRecvError::NoReply),
                },
                Err(SendError(msg)) => Err(SendRecvError::Closed(msg)),
            }
        })
    }
}

impl<'a, M, R> IntoRecv for SendFut<'a, M>
where
    M: Message<Type = Request<R>> + Send + 'a,
    R: Send + 'a,
{
    type Recv = Pin<Box<dyn Future<Output = Result<R, SendRecvError<M>>> + Send + 'a>>;

    fn into_recv(self) -> Self::Recv {
        Box::pin(async move {
            match self.await {
                Ok(rx) => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(_) => Err(SendRecvError::NoReply),
                },
                Err(SendError(msg)) => Err(SendRecvError::Closed(msg)),
            }
        })
    }
}

impl<M, R> IntoRecv for Result<Rx<R>, TrySendError<M>>
where
    M: Send + 'static,
    R: Send + 'static,
{
    type Recv = Pin<Box<dyn Future<Output = Result<R, TrySendRecvError<M>>> + Send + 'static>>;

    fn into_recv(self) -> Self::Recv {
        Box::pin(async move {
            match self {
                Ok(rx) => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(_) => Err(TrySendRecvError::NoReply),
                },
                Err(e) => match e {
                    TrySendError::Closed(msg) => Err(TrySendRecvError::Closed(msg)),
                    TrySendError::Full(msg) => Err(TrySendRecvError::Full(msg)),
                },
            }
        })
    }
}
