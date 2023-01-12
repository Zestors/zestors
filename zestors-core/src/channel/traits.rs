use crate::*;
use event_listener::EventListener;
use futures::{future::BoxFuture, Future};
use std::{
    any::{Any, TypeId},
    fmt::{Debug, Display},
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    sync::Arc,
};

//------------------------------------------------------------------------------------------------
//  Channel
//------------------------------------------------------------------------------------------------

pub trait Channel {
    fn close(&self) -> bool;
    fn halt_some(&self, n: u32);
    fn halt(&self);
    fn process_count(&self) -> usize;
    fn msg_count(&self) -> usize;
    fn address_count(&self) -> usize;
    fn is_closed(&self) -> bool;
    fn capacity(&self) -> &Capacity;
    fn has_exited(&self) -> bool;
    fn add_address(&self) -> usize;
    fn remove_address(&self) -> usize;
    fn get_exit_listener(&self) -> EventListener;
    fn actor_id(&self) -> ActorId;
    fn try_add_inbox(&self) -> Result<usize, ()>;
}

//------------------------------------------------------------------------------------------------
//  DynChannel
//------------------------------------------------------------------------------------------------

pub trait DynChannel: Channel + Send + Sync + Debug {
    fn try_send_boxed(
        &self,
        boxed: BoxedMessage,
    ) -> Result<(), TrySendUncheckedError<BoxedMessage>>;
    fn send_now_boxed(
        &self,
        boxed: BoxedMessage,
    ) -> Result<(), TrySendUncheckedError<BoxedMessage>>;
    fn send_blocking_boxed(
        &self,
        boxed: BoxedMessage,
    ) -> Result<(), SendUncheckedError<BoxedMessage>>;
    fn send_boxed(
        &self,
        boxed: BoxedMessage,
    ) -> BoxFuture<'_, Result<(), SendUncheckedError<BoxedMessage>>>;
    fn accepts(&self, id: &TypeId) -> bool;
    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

impl dyn DynChannel {
    pub(crate) fn try_send_unchecked<M>(
        &self,
        msg: M,
    ) -> Result<Returned<M>, TrySendUncheckedError<M>>
    where
        M: Message,
        Sent<M>: Send + 'static,
    {
        let (sends, returns) = <M::Type as MessageType<M>>::create(msg);
        let res = self.try_send_boxed(BoxedMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendUncheckedError::Full(boxed) => Err(TrySendUncheckedError::Full(
                    boxed.downcast_cancel(returns).unwrap(),
                )),
                TrySendUncheckedError::Closed(boxed) => Err(TrySendUncheckedError::Closed(
                    boxed.downcast_cancel(returns).unwrap(),
                )),
                TrySendUncheckedError::NotAccepted(boxed) => Err(
                    TrySendUncheckedError::NotAccepted(boxed.downcast_cancel(returns).unwrap()),
                ),
            },
        }
    }

    pub(crate) fn send_now_unchecked<M>(
        &self,
        msg: M,
    ) -> Result<Returned<M>, TrySendUncheckedError<M>>
    where
        M: Message,
        Sent<M>: Send + 'static,
    {
        let (sends, returns) = <M::Type as MessageType<M>>::create(msg);
        let res = self.send_now_boxed(BoxedMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendUncheckedError::Full(boxed) => Err(TrySendUncheckedError::Full(
                    boxed.downcast_cancel(returns).unwrap(),
                )),
                TrySendUncheckedError::Closed(boxed) => Err(TrySendUncheckedError::Closed(
                    boxed.downcast_cancel(returns).unwrap(),
                )),
                TrySendUncheckedError::NotAccepted(boxed) => Err(
                    TrySendUncheckedError::NotAccepted(boxed.downcast_cancel(returns).unwrap()),
                ),
            },
        }
    }

    pub(crate) fn send_blocking_unchecked<M>(
        &self,
        msg: M,
    ) -> Result<Returned<M>, SendUncheckedError<M>>
    where
        M: Message,
        Sent<M>: Send + 'static,
    {
        let (sends, returns) = <M::Type as MessageType<M>>::create(msg);
        let res = self.send_blocking_boxed(BoxedMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                SendUncheckedError::Closed(boxed) => Err(SendUncheckedError::Closed(
                    boxed.downcast_cancel(returns).unwrap(),
                )),
                SendUncheckedError::NotAccepted(boxed) => Err(SendUncheckedError::NotAccepted(
                    boxed.downcast_cancel(returns).unwrap(),
                )),
            },
        }
    }

    pub(crate) async fn send_unchecked<M>(
        &self,
        msg: M,
    ) -> Result<Returned<M>, SendUncheckedError<M>>
    where
        M: Message,
        Sent<M>: Send + 'static,
    {
        let (sends, returns) = <M::Type as MessageType<M>>::create(msg);
        let res = self.send_boxed(BoxedMessage::new::<M>(sends)).await;

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                SendUncheckedError::Closed(boxed) => Err(SendUncheckedError::Closed(
                    boxed.downcast_cancel(returns).unwrap(),
                )),
                SendUncheckedError::NotAccepted(boxed) => Err(SendUncheckedError::NotAccepted(
                    boxed.downcast_cancel(returns).unwrap(),
                )),
            },
        }
    }
}