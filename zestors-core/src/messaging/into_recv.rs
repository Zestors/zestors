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

