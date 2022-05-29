use std::time::Duration;

use futures::{Future, FutureExt};

//------------------------------------------------------------------------------------------------
//  Request
//------------------------------------------------------------------------------------------------

/// A request of type `R`, that must be replied to.
#[derive(Debug)]
pub struct Snd<R>(tokio::sync::oneshot::Sender<R>);

impl<R> Snd<R> {
    /// Create a new Request-Reply pair.
    pub fn new() -> (Snd<R>, Rcv<R>) {
        let (tx, rx) = tokio::sync::oneshot::channel();

        (Snd(tx), Rcv(rx))
    }

    /// Send a reply
    pub fn send(self, msg: R) -> Result<(), SendError<R>> {
        self.0.send(msg).map_err(|e| SendError(e))
    }

    /// Cancel this request. This is the same as dropping it.
    pub fn cancel(self) {}
}

/// Couldn't reply to this `Request`, because the `Reply` has been canceled/dropped.
#[derive(Debug)]
pub struct SendError<R>(pub R);

//------------------------------------------------------------------------------------------------
//  Reply
//------------------------------------------------------------------------------------------------

/// A reply of type `R`, that can be `.await`ed to retrieve the reply.
#[derive(Debug)]
pub struct Rcv<R>(tokio::sync::oneshot::Receiver<R>);

impl<R> Rcv<R> {
    /// Wait asynchronously for the reply. Instead of using this, it's simpler to `.await` the `Rcv`
    /// directly.
    pub async fn recv_async(self) -> Result<R, RcvError> {
        self.await
    }

    /// Check if there is already a reply
    pub fn try_recv(&mut self) -> Result<Option<R>, RcvError> {
        match self.0.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(e) => match e {
                tokio::sync::oneshot::error::TryRecvError::Empty => Ok(None),
                tokio::sync::oneshot::error::TryRecvError::Closed => Err(RcvError),
            },
        }
    }

    /// Blockingly wait for a reply
    // pub fn recv(self) -> Result<R, RcvError> {
    //     Ok(self.0.recv()?)
    // }

    // /// Blockingly wait for a reply, until the deadline.
    // pub fn recv_deadline(&self, deadline: std::time::Instant) -> Result<Option<R>, RcvError> {
    //     match self.0.recv_deadline(deadline) {
    //         Ok(msg) => Ok(Some(msg)),
    //         Err(e) => match e {
    //             oneshot::RecvTimeoutError::Timeout => Ok(None),
    //             oneshot::RecvTimeoutError::Disconnected => Err(RcvError),
    //         },
    //     }
    // }

    // /// Blockingly wait for a reply, until the timeout has passed.
    // pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<R>, RcvError> {
    //     match self.0.recv_timeout(timeout) {
    //         Ok(msg) => Ok(Some(msg)),
    //         Err(e) => match e {
    //             oneshot::RecvTimeoutError::Timeout => Ok(None),
    //             oneshot::RecvTimeoutError::Disconnected => Err(RcvError),
    //         },
    //     }
    // }

    /// Cancel this `Reply`. This is the same as dropping it.
    pub fn cancel(self) {}
}

impl<R> Future for Rcv<R> {
    type Output = Result<R, RcvError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.0.poll_unpin(cx).map_err(|e| e.into())
    }
}

impl<R> Unpin for Rcv<R> {}

/// Couldn't receive the `Reply`, because the `Request` has been canceled/dropped.
#[derive(Debug)]
pub struct RcvError;

//------------------------------------------------------------------------------------------------
//  From<oneshot::Error> implementations
//------------------------------------------------------------------------------------------------

// impl<R> From<tokio::sync::oneshot::error::<R>> for SendError<R> {
//     fn from(e: oneshot::SendError<R>) -> Self {
//         Self(e.into_inner())
//     }
// }

impl From<tokio::sync::oneshot::error::RecvError> for RcvError {
    fn from(_: tokio::sync::oneshot::error::RecvError) -> Self {
        Self
    }
}
