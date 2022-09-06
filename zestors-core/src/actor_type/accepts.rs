use crate::*;
use tiny_actor::{SendError, TrySendError};

/// Whether an actor accepts messages of a certain kind. If this is implemented for the
/// [ActorType] then messages of type `M` can be sent to it's address.
pub trait Accepts<M: Message>: ActorType {
    fn try_send(address: &Self::Channel, msg: M) -> Result<ReturnPart<M>, TrySendError<M>>;
    fn send_now(address: &Self::Channel, msg: M) -> Result<ReturnPart<M>, TrySendError<M>>;
    fn send_blocking(address: &Self::Channel, msg: M) -> Result<ReturnPart<M>, SendError<M>>;
    fn send(address: &Self::Channel, msg: M) -> SendFut<'_, M>;
}

impl<M, T> Accepts<M> for Dyn<T>
where
    Self: IntoDynamic<Dyn<dyn AcceptsOne<M>>>,
    M: Message + Send + 'static,
    SendPart<M>: Send + 'static,
    ReturnPart<M>: Send,
    T: ?Sized,
{
    fn try_send(address: &Self::Channel, msg: M) -> Result<ReturnPart<M>, TrySendError<M>> {
        address.try_send_unchecked(msg).map_err(|e| match e {
            TrySendUncheckedError::Full(msg) => TrySendError::Full(msg),
            TrySendUncheckedError::Closed(msg) => TrySendError::Closed(msg),
            TrySendUncheckedError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    fn send_now(address: &Self::Channel, msg: M) -> Result<ReturnPart<M>, TrySendError<M>> {
        address.send_now_unchecked(msg).map_err(|e| match e {
            TrySendUncheckedError::Full(msg) => TrySendError::Full(msg),
            TrySendUncheckedError::Closed(msg) => TrySendError::Closed(msg),
            TrySendUncheckedError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    fn send_blocking(address: &Self::Channel, msg: M) -> Result<ReturnPart<M>, SendError<M>> {
        address.send_blocking_unchecked(msg).map_err(|e| match e {
            SendUncheckedError::Closed(msg) => SendError(msg),
            SendUncheckedError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    fn send(address: &Self::Channel, msg: M) -> SendFut<'_, M> {
        SendFut(Box::pin(async move {
            address.send_unchecked(msg).await.map_err(|e| match e {
                SendUncheckedError::Closed(msg) => SendError(msg),
                SendUncheckedError::NotAccepted(_) => {
                    panic!("Sent message which was not accepted by actor")
                }
            })
        }))
    }
}

impl<M, P> Accepts<M> for P
where
    P: Protocol + ProtocolMessage<M>,
    ReturnPart<M>: Send,
    SendPart<M>: Send + 'static,
    M: Message + Send + 'static,
{
    fn try_send(address: &Self::Channel, msg: M) -> Result<ReturnPart<M>, TrySendError<M>> {
        let (sends, returns) = <M::Type as MessageType<M>>::create(msg);

        match address.try_send(P::from_sends(sends)) {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendError::Closed(prot) => {
                    Err(TrySendError::Closed(prot.unwrap_into_msg(returns)))
                }
                TrySendError::Full(prot) => Err(TrySendError::Full(prot.unwrap_into_msg(returns))),
            },
        }
    }

    fn send_now(address: &Self::Channel, msg: M) -> Result<ReturnPart<M>, TrySendError<M>> {
        let (sends, returns) = <M::Type as MessageType<M>>::create(msg);

        match address.send_now(P::from_sends(sends)) {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendError::Closed(prot) => {
                    Err(TrySendError::Closed(prot.unwrap_into_msg(returns)))
                }
                TrySendError::Full(prot) => Err(TrySendError::Full(prot.unwrap_into_msg(returns))),
            },
        }
    }

    fn send_blocking(address: &Self::Channel, msg: M) -> Result<ReturnPart<M>, SendError<M>> {
        let (sends, returns) = <M::Type as MessageType<M>>::create(msg);

        match address.send_blocking(P::from_sends(sends)) {
            Ok(()) => Ok(returns),
            Err(SendError(prot)) => Err(SendError(prot.unwrap_into_msg(returns))),
        }
    }

    fn send(address: &Self::Channel, msg: M) -> SendFut<'_, M> {
        SendFut(Box::pin(async move {
            let (sends, returns) = <M::Type as MessageType<M>>::create(msg);

            match address.send(P::from_sends(sends)).await {
                Ok(()) => Ok(returns),
                Err(SendError(prot)) => Err(SendError(prot.unwrap_into_msg(returns))),
            }
        }))
    }
}
