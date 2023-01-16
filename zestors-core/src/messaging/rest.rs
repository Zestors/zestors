use std::pin::Pin;

use futures::{Future, future::BoxFuture};
use thiserror::Error;
use super::*;

pub trait IntoRecv {
    type Returns;
    type Error;
    type Future: Future<Output = Result<Self::Returns, Self::Error>>;

    /// Wait for the reply of a request.
    fn into_recv(self) -> Self::Future;
}

impl<M, F, R, E> IntoRecv for Result<F, SendError<M>>
where
    M: Send + 'static,
    F: Future<Output = Result<R, E>> + Send + 'static,
    E: Into<SendRecvError<M>>,
    R: Send + 'static,
{
    type Returns = R;
    type Error = SendRecvError<M>;
    type Future = Pin<Box<dyn Future<Output = Result<R, SendRecvError<M>>> + Send + 'static>>;

    fn into_recv(self) -> Self::Future {
        Box::pin(async move {
            match self {
                Ok(rx) => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(e) => Err(e.into()),
                },
                Err(SendError(msg)) => Err(SendRecvError::Closed(msg)),
            }
        })
    }
}

impl<M, F, R, E> IntoRecv for Result<F, TrySendError<M>>
where
    M: Send + 'static,
    F: Future<Output = Result<R, E>> + Send + 'static,
    E: Into<TrySendRecvError<M>>,
    R: Send + 'static,
{
    type Returns = R;
    type Error = TrySendRecvError<M>;
    type Future = Pin<Box<dyn Future<Output = Result<R, TrySendRecvError<M>>> + Send + 'static>>;

    fn into_recv(self) -> Self::Future {
        Box::pin(async move {
            match self {
                Ok(rx) => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(e) => Err(e.into()),
                },
                Err(e) => match e {
                    TrySendError::Closed(msg) => Err(TrySendRecvError::Closed(msg)),
                    TrySendError::Full(msg) => Err(TrySendRecvError::Full(msg)),
                },
            }
        })
    }
}

impl<'a, M, F, R, E> IntoRecv for BoxFuture<'a, Result<F, SendError<M>>>
where
    M: Message<Returned = F> + Send + 'a,
    F: Future<Output = Result<R, E>> + Send + 'static,
    E: Into<SendRecvError<M>>,
    R: Send + 'a,
{
    type Returns = R;
    type Error = SendRecvError<M>;
    type Future = Pin<Box<dyn Future<Output = Result<R, SendRecvError<M>>> + Send + 'a>>;

    fn into_recv(self) -> Self::Future {
        Box::pin(async move {
            match self.await {
                Ok(rx) => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(e) => Err(e.into()),
                },
                Err(SendError(msg)) => Err(SendRecvError::Closed(msg)),
            }
        })
    }
}

/// Error returned when using the [IntoRecv] trait.
///
/// This error combines failures in sending and receiving.
#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum SendRecvError<M> {
    NoReply,
    Closed(M),
}

/// Error returned when using the [IntoRecv] trait.
///
/// This error combines failures in sending and receiving.
#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum TrySendRecvError<M> {
    NoReply,
    Closed(M),
    Full(M),
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum TrySendUncheckedError<M> {
    Full(M),
    Closed(M),
    NotAccepted(M),
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum SendUncheckedError<M> {
    Closed(M),
    NotAccepted(M),
}

/// An error returned when trying to send a message into a channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum TrySendError<M> {
    /// The channel has been closed, and no longer accepts new messages.
    #[error("Couldn't send message because Channel is closed")]
    Closed(M),
    /// The channel is full.
    #[error("Couldn't send message because Channel is full")]
    Full(M),
}

/// Error returned when sending a message into a channel.
///
/// The channel has been closed, and no longer accepts new messages.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub struct SendError<M>(pub M);
