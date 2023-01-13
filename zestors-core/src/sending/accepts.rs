use crate::*;
use futures::{future::BoxFuture, Future, FutureExt};
use std::pin::Pin;

pub trait Accept<M: Message>: DefineChannel {
    type SendFut<'a>: Future<Output = Result<Returned<M>, SendError<M>>> + Send + 'a;
    fn try_send(channel: &Self::Channel, msg: M) -> Result<Returned<M>, TrySendError<M>>;
    fn send_now(channel: &Self::Channel, msg: M) -> Result<Returned<M>, TrySendError<M>>;
    fn send_blocking(channel: &Self::Channel, msg: M) -> Result<Returned<M>, SendError<M>>;
    fn send(channel: &Self::Channel, msg: M) -> Self::SendFut<'_>;
}

impl<M, D> Accept<M> for Dyn<D>
where
    Self: DefineDynChannel + TransformInto<Accepts![M]>,
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

