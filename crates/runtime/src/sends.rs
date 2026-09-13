use std::marker::PhantomData;

use type_sets::AsTypeSet;

use super::*;

/// Provides message-sending operations for a channel.
///
/// `Sends` exposes four levels of delivery semantics:
///
/// - [`Sends::send`] applies backpressure and asynchronously waits when the
///   channel is under load.
/// - [`Sends::try_send`] applies backpressure but never waits, returning
///   [`ClosedOrFull::Full`] when the channel is under backpressure.
/// - [`Sends::send_now`] checks whether the channel is open but ignores
///   backpressure.
/// - [`Sends::force_send`] ignores both backpressure and the channel status.
pub trait Sends<M: Message>: Sync {
    /// Sends a message, applying backpressure when the channel is under load.
    ///
    /// This method waits asynchronously while backpressure is active. It
    /// returns [`Closed`] if the channel is closed.
    ///
    /// Unlike [`Sends::try_send`], this method waits rather than returning
    /// immediately when backpressure is active.
    fn send(&self, msg: M) -> impl Future<Output = Result<M::Receipt, SendError<M>>> + Send;

    /// Attempts to send a message without waiting.
    ///
    /// Returns [`ClosedOrFull::Full`] if the channel has reached its
    /// backpressure limit, or [`ClosedOrFull::Closed`] if the channel is
    /// closed.
    ///
    /// Unlike [`Sends::send`], this method never waits for backpressure to
    /// subside.
    fn try_send(&self, msg: M) -> Result<M::Receipt, TrySendError<M>>;

    /// Sends a message immediately if the channel is open.
    ///
    /// This method ignores backpressure, but still checks whether the channel
    /// is accepting messages. It returns [`Closed`] if the channel is closed.
    ///
    /// Use [`Sends::force_send`] when the channel status should also be
    /// ignored.
    fn send_now(&self, msg: M) -> Result<M::Receipt, SendError<M>>;

    // /// Sends a message immediately, ignoring backpressure and channel status.
    // ///
    // /// This is the lowest-level sending operation. The message is queued even
    // /// when the channel is closed.
    // ///
    // /// If the underlying queue is at capacity, the message is dropped and the
    // /// implementation may log the overflow.
    // fn force_send(&self, msg: M) -> M::Receipt;

    /// Sends a message and waits for a reply.
    ///
    /// This is the same as [`Sends::send`] with [`MessageOutput::receive`] called on the result. The resulting value is therefore [`Message::Output`] instead of
    /// [`Message::Output`].
    fn request(&self, msg: M) -> impl Future<Output = Result<M::Output, RequestError<M>>> + Send {
        async move { Ok(self.send(msg).await?.wait().await?) }
    }
}

/// A private trait for implementation on [`ActorHandle`] only.
///
/// There is a blacket-implementation of [`Sends`] for all types that implement
/// [`ActorHandle`].
pub(crate) trait _Sends<M: Message>: Sync {
    fn _send(&self, msg: M)
    -> impl Future<Output = Result<M::Receipt, SendError<M>>> + Send;
    fn _try_send(&self, msg: M) -> Result<M::Receipt, TrySendError<M>>;
    fn _send_now(&self, msg: M) -> Result<M::Receipt, SendError<M>>;
    fn _request(&self, msg: M) -> impl Future<Output = Result<M::Output, RequestError<M>>> + Send {
        async move { Ok(self._send(msg).await?.wait().await?) }
    }
}

impl<M, H> Sends<M> for H
where
    H: ActorOps + Sync,
    M: Message,
    Channel<H::Ctx>: _Sends<M>,
{
    async fn send(&self, msg: M) -> Result<M::Receipt, SendError<M>> {
        self.handle()._send(msg).await
    }

    fn try_send(&self, msg: M) -> Result<M::Receipt, TrySendError<M>> {
        self.handle()._try_send(msg)
    }

    fn send_now(&self, msg: M) -> Result<M::Receipt, SendError<M>> {
        self.handle()._send_now(msg)
    }
}
