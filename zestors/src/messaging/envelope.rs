use super::{Message, RequestError, SendError, TrySendError};
use crate::all::*;
use futures::{future::BoxFuture, Future};
use std::sync::Arc;

/// A [`Message`] that is ready to be sent.
pub struct Envelope<'a, A: ActorType, M> {
    channel: &'a Arc<A::Channel>,
    msg: M,
}

impl<'a, A, M> Envelope<'a, A, M>
where
    A: ActorType + Accepts<M>,
    M: Message,
{
    /// Create a new envelope from the message and channel.
    pub fn new(channel: &'a Arc<A::Channel>, msg: M) -> Self {
        Self { channel, msg }
    }

    /// Take out the inner message.
    pub fn into_msg(self) -> M {
        self.msg
    }

    /// Attempt to send the message.
    ///
    /// See [`crate::messaging`] for more information.
    pub fn try_send(self) -> Result<M::Returned, TrySendError<M>> {
        <A as Accepts<M>>::try_send(&self.channel, self.msg)
    }

    /// Attempt to send the message.
    ///
    /// See [`crate::messaging`] for more information.
    pub fn force_send(self) -> Result<M::Returned, TrySendError<M>> {
        <A as Accepts<M>>::force_send(&self.channel, self.msg)
    }

    /// Attempt to send the message.
    ///
    /// See [`crate::messaging`] for more information.
    pub fn send_blocking(self) -> Result<M::Returned, SendError<M>> {
        <A as Accepts<M>>::send_blocking(&self.channel, self.msg)
    }

    /// Attempt to send the message.
    ///
    /// See [`crate::messaging`] for more information.
    pub fn send(self) -> <A as Accepts<M>>::SendFut<'a> {
        <A as Accepts<M>>::send(&self.channel, self.msg)
    }

    /// Attempt to send the message.
    ///
    /// See [`crate::messaging`] for more information.
    pub fn try_request<F, E, R>(self) -> BoxFuture<'a, Result<R, TryRequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
    {
        <A as AcceptsExt<M>>::try_request(&self.channel, self.msg)
    }

    /// Attempt to send the message.
    ///
    /// See [`crate::messaging`] for more information.
    pub fn force_request<F, E, R>(self) -> BoxFuture<'a, Result<R, TryRequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
    {
        <A as AcceptsExt<M>>::force_request(&self.channel, self.msg)
    }

    /// Attempt to send the message.
    ///
    /// See [`crate::messaging`] for more information.
    pub fn request<F, E, R>(self) -> BoxFuture<'a, Result<R, RequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
    {
        <A as AcceptsExt<M>>::request(&self.channel, self.msg)
    }
}
