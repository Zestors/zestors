use crate::*;
use event_listener::EventListener;
use futures::future::BoxFuture;
use std::{
    any::{Any, TypeId},
    fmt::Debug,
    sync::Arc,
};

/// This trait must be implemented for a channel.
pub trait Channel: Send + Sync + Debug {
    /// Convert the channel into a `dyn Channel`.
    fn into_dyn(self: Arc<Self>) -> Arc<dyn Channel>;
    /// Convert the channel into a `dyn Any + Send + Sync`.
    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;

    /// The id of the actor.
    fn actor_id(&self) -> ActorId;

    /// Halts all processes of the actor.
    fn halt(&self);
    /// Halt `n` processes of the actor.
    ///
    /// For channels that do not allow multiple processes, this can defer to `self.halt()`.
    fn halt_some(&self, n: u32);

    /// The amount of addresses of the actor.
    fn address_count(&self) -> usize;
    /// Removes an address from the actor. Returns the old address-count.
    fn decrement_address_count(&self) -> usize;
    /// Adds a new address to the actor. Returns the old address-count.
    fn increment_address_count(&self) -> usize;

    /// The amount of messages in the channel.
    ///
    /// For channels that do not accept messages, this can return `0`.
    fn msg_count(&self) -> usize;
    /// The capacity of this channel.
    ///
    /// For channels that do not accept messages, this can return a `Capacity::Bounded(0)`
    fn capacity(&self) -> Capacity;
    /// Closes the channel and returns `true` if the channel was not closed before.
    ///
    /// For channels that do not accept messages, this must return `false`.
    fn close(&self) -> bool;
    /// Whether the channel is closed.
    ///
    /// For channels that do not accept messages, this must return `true`.
    fn is_closed(&self) -> bool;

    /// Whether all processes of the actor have exited.
    fn has_exited(&self) -> bool;
    /// Get an event-listener for when the actor exits.
    fn get_exit_listener(&self) -> EventListener;

    /// The amount of processes of the actor.
    ///
    /// For channels that do not allow multiple processes, this can return `1` or `0`.
    fn process_count(&self) -> usize;
    /// Attempt to add another process to the channel. If successful, this returns the previous
    /// process-count.
    fn try_increment_process_count(&self) -> Result<usize, TryAddProcessError>;

    /// Whether the channel accepts the type-id of a given message.
    ///
    /// For channels that do not accept messages, this can return `false`.
    fn accepts(&self, id: &TypeId) -> bool;
    /// Try to send a payload to the actor.
    ///
    /// For channels that do not accept messages, this can fail with `NotAccepted`.
    fn try_send_box(&self, msg: BoxPayload) -> Result<(), TrySendCheckedError<BoxPayload>>;
    /// Try to force-send a message to the actor.
    ///
    /// For channels that do not accept messages, this can fail with `NotAccepted`.
    fn force_send_box(&self, msg: BoxPayload) -> Result<(), TrySendCheckedError<BoxPayload>>;
    /// Send a payload to the actor while blocking the thread when waiting for space.
    ///
    /// For channels that do not accept messages, this can fail with `NotAccepted`.
    fn send_box_blocking(&self, msg: BoxPayload) -> Result<(), SendCheckedError<BoxPayload>>;
    /// Send a payload to the actor waiting for space.
    ///
    /// For channels that do not accept messages, this can fail with `NotAccepted`.
    fn send_box(&self, msg: BoxPayload) -> BoxFuture<'_, Result<(), SendCheckedError<BoxPayload>>>;
}

impl dyn Channel {
    pub fn try_send_checked<M: Message>(
        &self,
        msg: M,
    ) -> Result<M::Returned, TrySendCheckedError<M>> {
        let (sends, returns) = M::create(msg);
        let res = self.try_send_box(BoxPayload::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendCheckedError::Full(boxed) => Err(TrySendCheckedError::Full(
                    boxed.downcast_and_cancel(returns).unwrap(),
                )),
                TrySendCheckedError::Closed(boxed) => Err(TrySendCheckedError::Closed(
                    boxed.downcast_and_cancel(returns).unwrap(),
                )),
                TrySendCheckedError::NotAccepted(boxed) => Err(TrySendCheckedError::NotAccepted(
                    boxed.downcast_and_cancel(returns).unwrap(),
                )),
            },
        }
    }

    pub fn force_send_checked<M: Message>(
        &self,
        msg: M,
    ) -> Result<M::Returned, TrySendCheckedError<M>> {
        let (sends, returns) = M::create(msg);
        let res = self.force_send_box(BoxPayload::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendCheckedError::Full(boxed) => Err(TrySendCheckedError::Full(
                    boxed.downcast_and_cancel(returns).unwrap(),
                )),
                TrySendCheckedError::Closed(boxed) => Err(TrySendCheckedError::Closed(
                    boxed.downcast_and_cancel(returns).unwrap(),
                )),
                TrySendCheckedError::NotAccepted(boxed) => Err(TrySendCheckedError::NotAccepted(
                    boxed.downcast_and_cancel(returns).unwrap(),
                )),
            },
        }
    }

    pub fn send_blocking_checked<M: Message>(
        &self,
        msg: M,
    ) -> Result<M::Returned, SendCheckedError<M>> {
        let (sends, returns) = M::create(msg);
        let res = self.send_box_blocking(BoxPayload::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                SendCheckedError::Closed(boxed) => Err(SendCheckedError::Closed(
                    boxed.downcast_and_cancel(returns).unwrap(),
                )),
                SendCheckedError::NotAccepted(boxed) => Err(SendCheckedError::NotAccepted(
                    boxed.downcast_and_cancel(returns).unwrap(),
                )),
            },
        }
    }

    pub fn send_checked<M>(&self, msg: M) -> BoxFuture<'_, Result<M::Returned, SendCheckedError<M>>>
    where
        M::Returned: Send,
        M: Message + Send + 'static,
    {
        Box::pin(async move {
            let (sends, returns) = M::create(msg);
            let res = self.send_box(BoxPayload::new::<M>(sends)).await;

            match res {
                Ok(()) => Ok(returns),
                Err(e) => match e {
                    SendCheckedError::Closed(boxed) => Err(SendCheckedError::Closed(
                        boxed.downcast_and_cancel(returns).unwrap(),
                    )),
                    SendCheckedError::NotAccepted(boxed) => Err(SendCheckedError::NotAccepted(
                        boxed.downcast_and_cancel(returns).unwrap(),
                    )),
                },
            }
        })
    }
}
