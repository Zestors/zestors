use crate::*;
use event_listener::EventListener;
use futures::future::BoxFuture;
use std::{
    any::{Any, TypeId},
    fmt::Debug,
    sync::Arc,
};

/// The channel trait that must be implemented for all channel types.
///
/// Depending on the type of channel, certain methods may have mock-implementations.
/// This is described in those methods.
pub trait Channel: Send + Sync + Debug {
    /// Convert the channel into a `dyn Channel`.
    fn into_dyn(self: Arc<Self>) -> Arc<dyn Channel>;

    /// Convert the channel into a `dyn Any + Send + Sync`.
    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;

    /// Halts all processes of the actor.
    fn halt(&self);

    /// The amount of addresses of the actor.
    fn address_count(&self) -> usize;

    /// Whether all processes of the actor have exited.
    fn has_exited(&self) -> bool;

    /// Adds a new address to the actor.
    fn add_address(&self) -> usize;

    /// Removes an address from the actor.
    fn remove_address(&self) -> usize;

    /// Get an event-listener for when the actor exits.
    fn get_exit_listener(&self) -> EventListener;

    /// The id of the actor.
    fn actor_id(&self) -> ActorId;

    /// Closes the channel and returns `true` if the channel was not closed before.
    ///
    /// For channels that do not accept messages, this must return `false`.
    fn close(&self) -> bool;

    /// Whether the channel is closed.
    ///
    /// For channels that do not accept messages, this must return `true`.
    fn is_closed(&self) -> bool;

    /// Whether the channel accepts the type-id of a given message.
    ///
    /// For channels that do not accept messages, this must return `false`.
    fn accepts(&self, id: &TypeId) -> bool;

    /// The amount of messages in the channel.
    ///
    /// For channels that do not accept messages, this must return `0`.
    fn msg_count(&self) -> usize;

    /// The capacity of this channel.
    ///
    /// For channels that do not accept messages, this must return a `&'static Capacity::Bounded(0)`
    fn capacity(&self) -> &Capacity;

    /// The amount of processes of the actor.
    ///
    /// For channels that do not allow multiple processes, this must return `0`.
    fn process_count(&self) -> usize;

    /// Halt `n` processes of the actor.
    ///
    /// For channels that do not allow multiple processes, this must defer to `self.halt()`.
    fn halt_some(&self, n: u32);

    /// Attempt to add another process to the channel. If successful, this returns the previous
    /// process-count.
    ///
    /// This method must fail if:
    /// 1) The channel does not allow for multiple processes.
    /// 2) The actor has already exited.
    fn try_add_process(&self) -> Result<usize, TryAddProcessError>;

    /// Try to send a payload to the actor.
    ///
    /// For channels that do not accept messages, this must fail with `NotAccepted`.
    fn try_send_any(&self, msg: AnyPayload) -> Result<(), TrySendCheckedError<AnyPayload>>;

    /// Try to force-send a message to the actor.
    ///
    /// For channels that do not accept messages, this must fail with `NotAccepted`.
    fn force_send_any(&self, msg: AnyPayload) -> Result<(), TrySendCheckedError<AnyPayload>>;

    /// Send a payload to the actor while blocking the thread when waiting for space.
    ///
    /// For channels that do not accept messages, this must fail with `NotAccepted`.
    fn send_any_blocking(&self, msg: AnyPayload) -> Result<(), SendCheckedError<AnyPayload>>;

    /// Send a payload to the actor waiting for space.
    ///
    /// For channels that do not accept messages, this must fail with `NotAccepted`.
    fn send_any(&self, msg: AnyPayload) -> BoxFuture<'_, Result<(), SendCheckedError<AnyPayload>>>;
}

impl dyn Channel {
    pub fn try_send_checked<M>(&self, msg: M) -> Result<M::Returned, TrySendCheckedError<M>>
    where
        M: Message,
        M::Payload: Send + 'static,
    {
        let (sends, returns) = M::create(msg);
        let res = self.try_send_any(AnyPayload::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendCheckedError::Full(boxed) => Err(TrySendCheckedError::Full(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                TrySendCheckedError::Closed(boxed) => Err(TrySendCheckedError::Closed(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                TrySendCheckedError::NotAccepted(boxed) => Err(TrySendCheckedError::NotAccepted(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
            },
        }
    }

    pub fn force_send_unchecked<M>(&self, msg: M) -> Result<M::Returned, TrySendCheckedError<M>>
    where
        M: Message,
        M::Payload: Send + 'static,
    {
        let (sends, returns) = M::create(msg);
        let res = self.force_send_any(AnyPayload::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendCheckedError::Full(boxed) => Err(TrySendCheckedError::Full(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                TrySendCheckedError::Closed(boxed) => Err(TrySendCheckedError::Closed(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
                TrySendCheckedError::NotAccepted(boxed) => Err(TrySendCheckedError::NotAccepted(
                    boxed.downcast_then_cancel(returns).unwrap(),
                )),
            },
        }
    }

    pub fn send_blocking_checked<M>(&self, msg: M) -> Result<M::Returned, SendCheckedError<M>>
    where
        M: Message,
        M::Payload: Send + 'static,
    {
        let (sends, returns) = M::create(msg);
        let res = self.send_any_blocking(AnyPayload::new::<M>(sends));

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

    pub fn send_checked<M>(&self, msg: M) -> BoxFuture<'_, Result<M::Returned, SendCheckedError<M>>>
    where
        M::Returned: Send,
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
    {
        Box::pin(async move {
            let (sends, returns) = M::create(msg);
            let res = self.send_any(AnyPayload::new::<M>(sends)).await;

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

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TryAddProcessError {
    #[error("The actor has exited so no more processes may be added.")]
    ActorHasExited,
    #[error("The actor has a channel that does not support multiple inboxes.")]
    SingleInboxOnly,
}
