use crate::all::*;
use futures::{future::BoxFuture, Future};

/// [`Accepts`] is implemented for any [`ActorType`] that accepts the [`Message`] `M`.
pub trait Accepts<M: Message>: ActorType {
    type SendFut<'a>: Future<Output = Result<M::Returned, SendError<M>>> + Send;
    fn try_send(channel: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>>;
    fn force_send(channel: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>>;
    fn send_blocking(channel: &Self::Channel, msg: M) -> Result<M::Returned, SendError<M>>;
    fn send(channel: &Self::Channel, msg: M) -> Self::SendFut<'_>;
}

/// Automatically implemented methods for any type that implements [`Accepts`]
pub trait AcceptsExt<M: Message>: Accepts<M> {
    fn try_request<F, E, R>(
        channel: &Self::Channel,
        msg: M,
    ) -> BoxFuture<'_, Result<R, TryRequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
    {
        Box::pin(async move {
            match Self::try_send(channel, msg) {
                Ok(rx) => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(e) => Err(TryRequestError::NoReply(e)),
                },
                Err(TrySendError::Closed(msg)) => Err(TryRequestError::Closed(msg)),
                Err(TrySendError::Full(msg)) => Err(TryRequestError::Full(msg)),
            }
        })
    }

    fn force_request<F, E, R>(
        channel: &Self::Channel,
        msg: M,
    ) -> BoxFuture<'_, Result<R, TryRequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
    {
        Box::pin(async move {
            match Self::force_send(channel, msg) {
                Ok(rx) => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(e) => Err(TryRequestError::NoReply(e)),
                },
                Err(TrySendError::Closed(msg)) => Err(TryRequestError::Closed(msg)),
                Err(TrySendError::Full(msg)) => Err(TryRequestError::Full(msg)),
            }
        })
    }

    fn request<F, E, R>(
        channel: &Self::Channel,
        msg: M,
    ) -> BoxFuture<'_, Result<R, RequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
    {
        Box::pin(async move {
            match Self::send(channel, msg).await {
                Ok(rx) => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(e) => Err(RequestError::NoReply(e)),
                },
                Err(SendError(msg)) => Err(RequestError::Closed(msg)),
            }
        })
    }
}
impl<M: Message, T> AcceptsExt<M> for T where T: Accepts<M> {}
