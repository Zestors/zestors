use crate::*;
use futures::{future::BoxFuture, Future, FutureExt};
use std::pin::Pin;

/// Whether an actor accepts messages of a certain kind. If this is implemented for the
/// [ActorType] then messages of type `M` can be sent to it's address.
pub trait Accepts<M: Message>: DefinesChannel {
    type SendFut<'a>: Future<Output = Result<Returned<M>, SendError<M>>> + Send + 'a;
    fn try_send(channel: &Self::Channel, msg: M) -> Result<Returned<M>, TrySendError<M>>;
    fn send_now(channel: &Self::Channel, msg: M) -> Result<Returned<M>, TrySendError<M>>;
    fn send_blocking(channel: &Self::Channel, msg: M) -> Result<Returned<M>, SendError<M>>;
    fn send(channel: &Self::Channel, msg: M) -> Self::SendFut<'_>;
}

impl<M, D> Accepts<M> for Dyn<D>
where
    Self: DefinesDynChannel + TransformInto<Accepts![M]>,
    M: Message + Send + 'static,
    Sent<M>: Send,
    Returned<M>: Send,
    D: ?Sized,
{
    fn try_send(channel: &Self::Channel, msg: M) -> Result<Returned<M>, TrySendError<M>> {
        channel.try_send_unchecked(msg).map_err(|e| match e {
            TrySendUncheckedError::Full(msg) => TrySendError::Full(msg),
            TrySendUncheckedError::Closed(msg) => TrySendError::Closed(msg),
            TrySendUncheckedError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    fn send_now(channel: &Self::Channel, msg: M) -> Result<Returned<M>, TrySendError<M>> {
        channel.send_now_unchecked(msg).map_err(|e| match e {
            TrySendUncheckedError::Full(msg) => TrySendError::Full(msg),
            TrySendUncheckedError::Closed(msg) => TrySendError::Closed(msg),
            TrySendUncheckedError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    fn send_blocking(channel: &Self::Channel, msg: M) -> Result<Returned<M>, SendError<M>> {
        channel.send_blocking_unchecked(msg).map_err(|e| match e {
            SendUncheckedError::Closed(msg) => SendError(msg),
            SendUncheckedError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    type SendFut<'a> = BoxFuture<'a, Result<Returned<M>, SendError<M>>>;

    fn send(channel: &Self::Channel, msg: M) -> Self::SendFut<'_> {
        Box::pin(async move {
            channel.send_unchecked(msg).await.map_err(|e| match e {
                SendUncheckedError::Closed(msg) => SendError(msg),
                SendUncheckedError::NotAccepted(_) => {
                    panic!("Sent message which was not accepted by actor")
                }
            })
        })
    }
}

impl<M, P> Accepts<M> for P
where
    P: Protocol + ProtocolAccepts<M>,
    M: Message + Send + 'static,
    Sent<M>: Send,
    Returned<M>: Send,
{
    fn try_send(address: &Self::Channel, msg: M) -> Result<Returned<M>, TrySendError<M>> {
        let (sends, returns) = <M::Type as MessageType<M>>::create(msg);

        match address.try_send_raw(P::from_msg(sends)) {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendError::Closed(prot) => {
                    Err(TrySendError::Closed(prot.unwrap_and_cancel(returns)))
                }
                TrySendError::Full(prot) => {
                    Err(TrySendError::Full(prot.unwrap_and_cancel(returns)))
                }
            },
        }
    }

    fn send_now(address: &Self::Channel, msg: M) -> Result<Returned<M>, TrySendError<M>> {
        let (sends, returns) = <M::Type as MessageType<M>>::create(msg);

        match address.send_raw_now(P::from_msg(sends)) {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendError::Closed(prot) => {
                    Err(TrySendError::Closed(prot.unwrap_and_cancel(returns)))
                }
                TrySendError::Full(prot) => {
                    Err(TrySendError::Full(prot.unwrap_and_cancel(returns)))
                }
            },
        }
    }

    fn send_blocking(address: &Self::Channel, msg: M) -> Result<Returned<M>, SendError<M>> {
        let (sends, returns) = <M::Type as MessageType<M>>::create(msg);

        match address.send_raw_blocking(P::from_msg(sends)) {
            Ok(()) => Ok(returns),
            Err(SendError(prot)) => Err(SendError(prot.unwrap_and_cancel(returns))),
        }
    }

    type SendFut<'a> = BoxFuture<'a, Result<Returned<M>, SendError<M>>>;

    fn send(address: &Self::Channel, msg: M) -> Self::SendFut<'_> {
        Box::pin(async move {
            let (sends, returns) = <M::Type as MessageType<M>>::create(msg);

            match address.send_raw(P::from_msg(sends)).await {
                Ok(()) => Ok(returns),
                Err(SendError(prot)) => Err(SendError(prot.unwrap_and_cancel(returns))),
            }
        })
    }
}

/// Future returned when sending a message to an [Address].
pub struct BoxSendFut<'a, M: Message>(
    pub(crate) Pin<Box<dyn Future<Output = Result<Returned<M>, SendError<M>>> + Send + 'a>>,
);

impl<'a, M: Message> Unpin for BoxSendFut<'a, M> {}

impl<'a, M: Message> Future for BoxSendFut<'a, M> {
    type Output = Result<Returned<M>, SendError<M>>;

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.0.poll_unpin(cx)
    }
}
