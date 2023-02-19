use std::sync::Arc;

use futures::{future::BoxFuture, Future};

use super::{request::Rx, Message, RequestError, SendError, TrySendError};
use crate::all::*;

pub struct Envelope<'a, A: ActorType, M> {
    channel: &'a Arc<A::Channel>,
    msg: M,
}

impl<'a, A, M> Envelope<'a, A, M>
where
    A: ActorType + Accept<M>,
    M: Message,
{
    pub fn new(channel: &'a Arc<A::Channel>, msg: M) -> Self {
        Self { channel, msg }
    }

    pub fn into_msg(self) -> M {
        self.msg
    }

    pub fn try_send(self) -> Result<M::Returned, TrySendError<M>> {
        <A as Accept<M>>::try_send(&self.channel, self.msg)
    }

    pub fn force_send(self) -> Result<M::Returned, TrySendError<M>> {
        <A as Accept<M>>::force_send(&self.channel, self.msg)
    }

    pub fn send_blocking(self) -> Result<M::Returned, SendError<M>> {
        <A as Accept<M>>::send_blocking(&self.channel, self.msg)
    }

    pub fn send(self) -> <A as Accept<M>>::SendFut<'a> {
        <A as Accept<M>>::send(&self.channel, self.msg)
    }

    pub fn try_request<F, E, R>(self) -> BoxFuture<'a, Result<R, TryRequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
    {
        <A as AcceptExt<M>>::try_request(&self.channel, self.msg)
    }

    pub fn force_request<F, E, R>(self) -> BoxFuture<'a, Result<R, TryRequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
    {
        <A as AcceptExt<M>>::force_request(&self.channel, self.msg)
    }

    pub fn request<F, E, R>(self) -> BoxFuture<'a, Result<R, RequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
    {
        <A as AcceptExt<M>>::request(&self.channel, self.msg)
    }
}
