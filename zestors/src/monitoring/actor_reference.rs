use crate::*;
use futures::{future::BoxFuture, Future};
use std::{any::TypeId, sync::Arc};

/// Implemented for any reference to an actor, which allows interaction with the actor.
///
/// See [`ChannelRefExt`] and [`DynChannelRefExt`] for callable methods.
pub trait ActorRef: Sized {
    type ActorType: ActorType;
    fn channel_ref(&self) -> &Arc<<Self::ActorType as ActorType>::Channel>;
}

pub trait ActorRefExt: ActorRef {
    fn envelope<M>(&self, msg: M) -> Envelope<'_, Self::ActorType, M>
    where
        M: Message,
        Self::ActorType: Accept<M>,
    {
        Envelope::new(self.channel_ref(), msg)
    }

    fn get_address(&self) -> Address<Self::ActorType> {
        let channel = self.channel_ref().clone();
        channel.increment_address_count();
        Address::from_channel(channel)
    }

    fn is_bounded(&self) -> bool {
        self.capacity().is_bounded()
    }

    fn close(&self) -> bool {
        <Self as ActorRef>::channel_ref(self).close()
    }

    fn halt_some(&self, n: u32) {
        <Self as ActorRef>::channel_ref(self).halt_some(n)
    }

    fn halt(&self) {
        <Self as ActorRef>::channel_ref(self).halt_some(u32::MAX)
    }

    fn process_count(&self) -> usize {
        <Self as ActorRef>::channel_ref(self).process_count()
    }

    fn msg_count(&self) -> usize {
        <Self as ActorRef>::channel_ref(self).msg_count()
    }

    fn address_count(&self) -> usize {
        <Self as ActorRef>::channel_ref(self).address_count()
    }

    fn is_closed(&self) -> bool {
        <Self as ActorRef>::channel_ref(self).is_closed()
    }

    fn capacity(&self) -> Capacity {
        <Self as ActorRef>::channel_ref(self).capacity()
    }

    fn has_exited(&self) -> bool {
        <Self as ActorRef>::channel_ref(self).has_exited()
    }

    fn actor_id(&self) -> ActorId {
        <Self as ActorRef>::channel_ref(self).actor_id()
    }

    fn try_send<M>(&self, msg: M) -> Result<M::Returned, TrySendError<M>>
    where
        M: Message,
        Self::ActorType: Accept<M>,
    {
        <Self::ActorType as Accept<M>>::try_send(self.channel_ref(), msg)
    }

    fn force_send<M>(&self, msg: M) -> Result<M::Returned, TrySendError<M>>
    where
        M: Message,
        Self::ActorType: Accept<M>,
    {
        <Self::ActorType as Accept<M>>::force_send(self.channel_ref(), msg)
    }

    fn send_blocking<M>(&self, msg: M) -> Result<M::Returned, SendError<M>>
    where
        M: Message,
        Self::ActorType: Accept<M>,
    {
        <Self::ActorType as Accept<M>>::send_blocking(self.channel_ref(), msg)
    }

    fn send<M>(&self, msg: M) -> <Self::ActorType as Accept<M>>::SendFut<'_>
    where
        M: Message,
        Self::ActorType: Accept<M>,
    {
        <Self::ActorType as Accept<M>>::send(self.channel_ref(), msg)
    }

    fn try_request<M, F, E, R>(&self, msg: M) -> BoxFuture<'_, Result<R, TryRequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
        Self::ActorType: Accept<M>,
    {
        <Self::ActorType as AcceptExt<M>>::try_request(self.channel_ref(), msg)
    }

    fn force_request<M, F, E, R>(&self, msg: M) -> BoxFuture<'_, Result<R, TryRequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
        Self::ActorType: Accept<M>,
    {
        <Self::ActorType as AcceptExt<M>>::force_request(self.channel_ref(), msg)
    }

    fn request<M, F, E, R>(&self, msg: M) -> BoxFuture<'_, Result<R, RequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
        Self::ActorType: Accept<M>,
    {
        <Self::ActorType as AcceptExt<M>>::request(self.channel_ref(), msg)
    }
}
impl<T> ActorRefExt for T where T: ActorRef {}

/// A specializition of an [`ActorRef`] that allows for sending messages which are checked at runtime
/// whether the actor actually [accepts](Accept) the message.
pub trait DynChannelReferenceExt: ActorRef
where
    Self::ActorType: DynActorType,
{
    fn try_send_checked<M>(&self, msg: M) -> Result<M::Returned, TrySendCheckedError<M>>
    where
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
    {
        <Self as ActorRef>::channel_ref(self).try_send_checked(msg)
    }
    fn force_send_unchecked<M>(&self, msg: M) -> Result<M::Returned, TrySendCheckedError<M>>
    where
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
    {
        <Self as ActorRef>::channel_ref(self).force_send_checked(msg)
    }
    fn send_blocking_checked<M>(&self, msg: M) -> Result<M::Returned, SendCheckedError<M>>
    where
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
    {
        <Self as ActorRef>::channel_ref(self).send_blocking_checked(msg)
    }
    fn send_checked<M>(&self, msg: M) -> BoxFuture<'_, Result<M::Returned, SendCheckedError<M>>>
    where
        M::Returned: Send,
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
    {
        <Self as ActorRef>::channel_ref(self).send_checked(msg)
    }
    fn accepts(&self, id: &TypeId) -> bool {
        <Self as ActorRef>::channel_ref(self).accepts(id)
    }
}

impl<T> DynChannelReferenceExt for T
where
    T: ActorRef,
    T::ActorType: DynActorType,
{
}
