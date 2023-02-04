use crate::*;
use event_listener::EventListener;
use futures::future::BoxFuture;
use std::{
    any::{Any, TypeId},
    fmt::Debug,
    sync::Arc,
};



/// Trait that all channels must implement.
pub trait Channel: Send + Sync + Debug {
    fn into_dyn(self: Arc<Self>) -> Arc<dyn Channel>;
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
    fn try_send_boxed(&self, boxed: AnyMessage) -> Result<(), TrySendCheckedError<AnyMessage>>;
    fn send_now_boxed(&self, boxed: AnyMessage) -> Result<(), TrySendCheckedError<AnyMessage>>;
    fn send_blocking_boxed(&self, boxed: AnyMessage) -> Result<(), SendCheckedError<AnyMessage>>;
    fn send_boxed(
        &self,
        boxed: AnyMessage,
    ) -> BoxFuture<'_, Result<(), SendCheckedError<AnyMessage>>>;
    fn accepts(&self, id: &TypeId) -> bool;
    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

impl dyn Channel {
    pub fn try_send_checked<M>(&self, msg: M) -> Result<M::Returned, TrySendCheckedError<M>>
    where
        M: Message,
        M::Payload: Send + 'static,
    {
        let (sends, returns) = M::create(msg);
        let res = self.try_send_boxed(AnyMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendCheckedError::Full(boxed) => Err(TrySendCheckedError::Full(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                TrySendCheckedError::Closed(boxed) => Err(TrySendCheckedError::Closed(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                TrySendCheckedError::NotAccepted(boxed) => {
                    Err(TrySendCheckedError::NotAccepted(
                        boxed.downcast_then_cancel(returns).unwrap(),
                    ))
                }
            },
        }
    }

    pub fn send_now_unchecked<M>(&self, msg: M) -> Result<M::Returned, TrySendCheckedError<M>>
    where
        M: Message,
        M::Payload: Send + 'static,
    {
        let (sends, returns) = M::create(msg);
        let res = self.send_now_boxed(AnyMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendCheckedError::Full(boxed) => Err(TrySendCheckedError::Full(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                TrySendCheckedError::Closed(boxed) => Err(TrySendCheckedError::Closed(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                TrySendCheckedError::NotAccepted(boxed) => {
                    Err(TrySendCheckedError::NotAccepted(
                        boxed.downcast_then_cancel(returns).unwrap(),
                    ))
                }
            },
        }
    }

    pub fn send_blocking_checked<M>(&self, msg: M) -> Result<M::Returned, SendCheckedError<M>>
    where
        M: Message,
        M::Payload: Send + 'static,
    {
        let (sends, returns) = M::create(msg);
        let res = self.send_blocking_boxed(AnyMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                SendCheckedError::Closed(boxed) => Err(SendCheckedError::Closed(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                SendCheckedError::NotAccepted(boxed) => Err(SendCheckedError::NotAccepted(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
            },
        }
    }

    pub fn send_checked<M>(
        &self,
        msg: M,
    ) -> BoxFuture<'_, Result<M::Returned, SendCheckedError<M>>>
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
                    SendCheckedError::Closed(boxed) => Err(SendCheckedError::Closed(
                        boxed.downcast_then_cancel(returns).unwrap(),
                    )),
                    SendCheckedError::NotAccepted(boxed) => Err(SendCheckedError::NotAccepted(
                        boxed.downcast_then_cancel(returns).unwrap(),
                    )),
                },
            }
        })
    }
}
