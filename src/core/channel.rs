use futures::{Future, FutureExt};

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
    /// Results in an error if the receiver has been dropped.
    pub fn send(self, msg: T) -> Result<(), SndError<T>> {
        self.0.send(msg).map_err(|e| SndError(e))
    }

    /// Cancel this channel. This is the same as dropping it.
    ///
    /// The receiver will get an error indicating the channel has been canceled.
    pub fn canceled(self) {}
}

/// Couldn't send the message, because the receiver has been canceled or dropped.
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
    /// * Returns `Err(RcvError)` if the sender has been dropped or canceled.
    pub fn try_recv(&mut self) -> Result<Option<R>, RcvError> {
        match self.0.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(e) => match e {
                tokio::sync::oneshot::error::TryRecvError::Empty => Ok(None),
                tokio::sync::oneshot::error::TryRecvError::Closed => Err(RcvError),
            },
        }
    }

    /// Cancel this channel. This is the same as dropping it.
    ///
    /// The sender will get an error indicating the channel has been canceled when it tries
    /// to send a message.
    ///
    /// If the sender has already sent a message, the message will be dropped.
    pub fn cancel(self) {}
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

/// Couldn't receive the `Reply`, because the `Request` has been canceled/dropped.
#[derive(Debug)]
pub struct RcvError;
