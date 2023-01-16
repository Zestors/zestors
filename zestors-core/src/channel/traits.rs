use crate::*;
use event_listener::EventListener;
use futures::future::BoxFuture;
use std::{
    any::{Any, TypeId},
    fmt::Debug,
    sync::Arc,
};

/// Trait that all channels must implement.
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

/// Trait that all dynamic channels must implement
pub trait DynChannel: Channel + Send + Sync + Debug {
    fn try_send_boxed(
        &self,
        boxed: AnyMessage,
    ) -> Result<(), TrySendUncheckedError<AnyMessage>>;
    fn send_now_boxed(
        &self,
        boxed: AnyMessage,
    ) -> Result<(), TrySendUncheckedError<AnyMessage>>;
    fn send_blocking_boxed(
        &self,
        boxed: AnyMessage,
    ) -> Result<(), SendUncheckedError<AnyMessage>>;
    fn send_boxed(
        &self,
        boxed: AnyMessage,
    ) -> BoxFuture<'_, Result<(), SendUncheckedError<AnyMessage>>>;
    fn accepts(&self, id: &TypeId) -> bool;
    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

impl dyn DynChannel {
    pub fn try_send_unchecked<M>(
        &self,
        msg: M,
    ) -> Result<M::Returned, TrySendUncheckedError<M>>
    where
        M: Message,
        M::Payload: Send + 'static,
    {
        let (sends, returns) = M::create(msg);
        let res = self.try_send_boxed(AnyMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendUncheckedError::Full(boxed) => Err(TrySendUncheckedError::Full(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                TrySendUncheckedError::Closed(boxed) => Err(TrySendUncheckedError::Closed(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                TrySendUncheckedError::NotAccepted(boxed) => Err(
                    TrySendUncheckedError::NotAccepted(boxed.downcast_then_cancel(returns).unwrap()),
                ),
            },
        }
    }

    pub fn send_now_unchecked<M>(
        &self,
        msg: M,
    ) -> Result<M::Returned, TrySendUncheckedError<M>>
    where
        M: Message,
        M::Payload: Send + 'static,
    {
        let (sends, returns) = M::create(msg);
        let res = self.send_now_boxed(AnyMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendUncheckedError::Full(boxed) => Err(TrySendUncheckedError::Full(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                TrySendUncheckedError::Closed(boxed) => Err(TrySendUncheckedError::Closed(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                TrySendUncheckedError::NotAccepted(boxed) => Err(
                    TrySendUncheckedError::NotAccepted(boxed.downcast_then_cancel(returns).unwrap()),
                ),
            },
        }
    }

    pub fn send_blocking_unchecked<M>(
        &self,
        msg: M,
    ) -> Result<M::Returned, SendUncheckedError<M>>
    where
        M: Message,
        M::Payload: Send + 'static,
    {
        let (sends, returns) = M::create(msg);
        let res = self.send_blocking_boxed(AnyMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                SendUncheckedError::Closed(boxed) => Err(SendUncheckedError::Closed(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                SendUncheckedError::NotAccepted(boxed) => Err(SendUncheckedError::NotAccepted(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
            },
        }
    }

    pub fn send_unchecked<M>(
        &self,
        msg: M,
    ) -> BoxFuture<'_, Result<M::Returned, SendUncheckedError<M>>>
    where
        M::Returned: Send,
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
    {
        Box::pin(async move {
            let (sends, returns) = M::create(msg);
            let res = self.send_boxed(AnyMessage::new::<M>(sends)).await;

            match res {
                Ok(()) => Ok(returns),
                Err(e) => match e {
                    SendUncheckedError::Closed(boxed) => Err(SendUncheckedError::Closed(
                        boxed.downcast_then_cancel(returns).unwrap(),
                    )),
                    SendUncheckedError::NotAccepted(boxed) => Err(SendUncheckedError::NotAccepted(
                        boxed.downcast_then_cancel(returns).unwrap(),
                    )),
                },
            }
        })
    }
}
