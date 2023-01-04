use crate::*;
use futures::Future;
use std::pin::Pin;

/// Trait that makes it easier to await a reply from a [Request].
pub trait IntoRecv {
    type Receives;

    /// Wait for the reply of a request.
    fn into_recv(self) -> Self::Receives;
}

impl<M, R> IntoRecv for Result<Rx<R>, SendError<M>>
where
    M: Send + 'static,
    R: Send + 'static,
{
    type Receives = Pin<Box<dyn Future<Output = Result<R, SendRecvError<M>>> + Send + 'static>>;

    fn into_recv(self) -> Self::Receives {
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
    M: Message<Type = Rx<R>> + Send + 'a,
    R: Send + 'a,
{
    type Receives = Pin<Box<dyn Future<Output = Result<R, SendRecvError<M>>> + Send + 'a>>;

    fn into_recv(self) -> Self::Receives {
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
    type Receives = Pin<Box<dyn Future<Output = Result<R, TrySendRecvError<M>>> + Send + 'static>>;

    fn into_recv(self) -> Self::Receives {
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
