use crate::core::*;
use futures::{Future, FutureExt};
use std::pin::Pin;

/// Creates a new oneshot channel, that allows for sending or receiving a single message.
pub fn new_channel<T>() -> (Snd<T>, Rcv<T>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    (Snd(tx), Rcv(rx))
}

//------------------------------------------------------------------------------------------------
//  Snd
//------------------------------------------------------------------------------------------------

/// A sender that allows for sending a single message of type `T`.
#[derive(Debug)]
pub struct Snd<T>(tokio::sync::oneshot::Sender<T>);

impl<T> Snd<T> {
    /// Send a message. Consumes the sender.
    ///
    /// Results in an error if the receiver has been closed.
    pub fn send(self, msg: T) -> Result<(), SndError<T>> {
        self.0.send(msg).map_err(|e| SndError(e))
    }

    /// Checks whether the receiver has closed/dropped this channel.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    /// Waits for the receiver to close/drop the sender.
    pub async fn closed(&mut self) {
        self.0.closed().await
    }
}

/// Couldn't send the message, because the receiver has been closed/dropped.
#[derive(Debug)]
pub struct SndError<R>(pub R);

//------------------------------------------------------------------------------------------------
//  Rcv
//------------------------------------------------------------------------------------------------

/// A receiver that allows for receiving a single message of type `T`.
///
/// Await this receiver to retrieve the message.
#[derive(Debug)]
pub struct Rcv<R>(tokio::sync::oneshot::Receiver<R>);

impl<R> Rcv<R> {
    /// Check if the reply is ready.
    /// * Returns `Ok(Some(R))` if there was a message ready.
    /// * Returns `Ok(None)` if there was no message yet.
    /// * Returns `Err(RcvError)` if the sender has been dropped.
    pub fn try_recv(&mut self) -> Result<Option<R>, RcvError> {
        match self.0.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(e) => match e {
                tokio::sync::oneshot::error::TryRecvError::Empty => Ok(None),
                tokio::sync::oneshot::error::TryRecvError::Closed => Err(RcvError),
            },
        }
    }

    /// Blocks the thread to wait for a reply. Can be useful when calling from a synchronous context,
    /// however this is heavily discouraged to be called from an asynchronous context. Instead,
    /// await the `Rcv` directly.
    pub fn recv_blocking(self) -> Result<R, RcvError> {
        Ok(self.0.blocking_recv()?)
    }

    /// Closes the channel to prevent a message from being sent.
    ///
    /// If a message was already sent, then this message can still be received.
    pub fn close(&mut self) {
        self.0.close()
    }
}

impl<R> Future for Rcv<R> {
    type Output = Result<R, RcvError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.0.poll_unpin(cx).map_err(|_| RcvError)
    }
}

impl<R> Unpin for Rcv<R> {}

/// Couldn't receive the message, because the sender has been dropped.
#[derive(Debug)]
pub struct RcvError;

impl From<tokio::sync::oneshot::error::RecvError> for RcvError {
    fn from(_: tokio::sync::oneshot::error::RecvError) -> Self {
        Self
    }
}

//------------------------------------------------------------------------------------------------
//  IntoRcv
//------------------------------------------------------------------------------------------------

/// Converts a `Result<Rcv<R>, LocalAddrError<M>>` into a future which can be awaited. The send
/// and receive errors will be combined in this case.
///
/// The output of the new future is `Result<R, SndRcvError<M>>`.
pub trait IntoRcv {
    type Output;
    fn into_rcv(self) -> Self::Output;
}

impl<M: Send + 'static, R: Send + 'static> IntoRcv for Result<Rcv<R>, AddrSndError<M>> {
    type Output = Pin<Box<dyn Future<Output = Result<R, SndRcvError<M>>> + Send + 'static>>;

    /// Converts a `Result<Rcv<R>, LocalAddrError<M>>` into a future which can be awaited. The send
    /// and receive errors will be combined in this case.
    ///
    /// The output of the new future is `Result<R, SndRcvError<M>>`.
    fn into_rcv(self) -> Pin<Box<dyn Future<Output = Result<R, SndRcvError<M>>> + Send + 'static>> {
        Box::pin(async move {
            match self {
                Ok(rcv) => match rcv.await {
                    Ok(r) => Ok(r),
                    Err(_e) => Err(SndRcvError::RcvFailure),
                },
                Err(e) => Err(SndRcvError::SndFailure(e.0)),
            }
        })
    }
}
