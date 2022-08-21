use crate::*;
use futures::Future;
use std::{any::TypeId, fmt::Debug, pin::Pin};
use tiny_actor::{Channel, TrySendError, SendError};

/// The internal channel-trait used within zestors, which allows for sending messages dynamically.
pub trait BoxChannel: tiny_actor::AnyChannel + tiny_actor::DynChannel + Debug {
    fn try_send_boxed(&self, boxed: BoxedMessage) -> Result<(), TrySendUncheckedError<BoxedMessage>>;
    fn send_now_boxed(&self, boxed: BoxedMessage) -> Result<(), TrySendUncheckedError<BoxedMessage>>;
    fn send_blocking_boxed(&self, boxed: BoxedMessage) -> Result<(), SendUncheckedError<BoxedMessage>>;
    fn send_boxed<'a>(&'a self, boxed: BoxedMessage) -> SendBoxedFut<'a>;
    fn accepts(&self, id: &TypeId) -> bool;
}

impl<P: Protocol + Send + 'static> BoxChannel for Channel<P> {
    fn try_send_boxed(&self, boxed: BoxedMessage) -> Result<(), TrySendUncheckedError<BoxedMessage>> {
        match P::try_from_boxed(boxed) {
            Ok(prot) => self.try_send(prot).map_err(|e| match e {
                TrySendError::Full(prot) => TrySendUncheckedError::Full(prot.into_boxed()),
                TrySendError::Closed(prot) => TrySendUncheckedError::Closed(prot.into_boxed()),
            }),
            Err(boxed) => Err(TrySendUncheckedError::NotAccepted(boxed)),
        }
    }

    fn send_now_boxed(&self, boxed: BoxedMessage) -> Result<(), TrySendUncheckedError<BoxedMessage>> {
        match P::try_from_boxed(boxed) {
            Ok(prot) => self.send_now(prot).map_err(|e| match e {
                TrySendError::Full(prot) => TrySendUncheckedError::Full(prot.into_boxed()),
                TrySendError::Closed(prot) => TrySendUncheckedError::Closed(prot.into_boxed()),
            }),
            Err(boxed) => Err(TrySendUncheckedError::NotAccepted(boxed)),
        }
    }

    fn send_blocking_boxed(&self, boxed: BoxedMessage) -> Result<(), SendUncheckedError<BoxedMessage>> {
        match P::try_from_boxed(boxed) {
            Ok(prot) => self
                .send_blocking(prot)
                .map_err(|SendError(prot)| SendUncheckedError::Closed(prot.into_boxed())),
            Err(boxed) => Err(SendUncheckedError::NotAccepted(boxed)),
        }
    }

    fn send_boxed<'a>(&'a self, boxed: BoxedMessage) -> SendBoxedFut<'a> {
        Box::pin(async move {
            match P::try_from_boxed(boxed) {
                Ok(prot) => self
                    .send(prot)
                    .await
                    .map_err(|SendError(prot)| SendUncheckedError::Closed(prot.into_boxed())),
                Err(boxed) => Err(SendUncheckedError::NotAccepted(boxed)),
            }
        })
    }

    fn accepts(&self, id: &TypeId) -> bool {
        <P as Protocol>::accepts(id)
    }
}

impl dyn BoxChannel {
    pub(crate) fn try_send_unchecked<M>(&self, msg: M) -> Result<Returns<M>, TrySendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        Sends<M>: Send + 'static,
    {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);
        let res = self.try_send_boxed(BoxedMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendUncheckedError::Full(boxed) => Err(TrySendUncheckedError::Full(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
                TrySendUncheckedError::Closed(boxed) => Err(TrySendUncheckedError::Closed(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
                TrySendUncheckedError::NotAccepted(boxed) => Err(TrySendUncheckedError::NotAccepted(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
            },
        }
    }

    pub(crate) fn send_now_unchecked<M>(&self, msg: M) -> Result<Returns<M>, TrySendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        Sends<M>: Send + 'static,
    {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);
        let res = self.send_now_boxed(BoxedMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendUncheckedError::Full(boxed) => Err(TrySendUncheckedError::Full(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
                TrySendUncheckedError::Closed(boxed) => Err(TrySendUncheckedError::Closed(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
                TrySendUncheckedError::NotAccepted(boxed) => Err(TrySendUncheckedError::NotAccepted(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
            },
        }
    }

    pub(crate) fn send_blocking_unchecked<M>(&self, msg: M) -> Result<Returns<M>, SendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        Sends<M>: Send + 'static,
    {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);
        let res = self.send_blocking_boxed(BoxedMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                SendUncheckedError::Closed(boxed) => Err(SendUncheckedError::Closed(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
                SendUncheckedError::NotAccepted(boxed) => Err(SendUncheckedError::NotAccepted(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
            },
        }
    }

    pub(crate) async fn send_unchecked<M>(&self, msg: M) -> Result<Returns<M>, SendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        Sends<M>: Send + 'static,
    {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);
        let res = self.send_boxed(BoxedMessage::new::<M>(sends)).await;

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                SendUncheckedError::Closed(boxed) => Err(SendUncheckedError::Closed(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
                SendUncheckedError::NotAccepted(boxed) => Err(SendUncheckedError::NotAccepted(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
            },
        }
    }
}

type SendBoxedFut<'a> =
    Pin<Box<dyn Future<Output = Result<(), SendUncheckedError<BoxedMessage>>> + Send + 'a>>;
