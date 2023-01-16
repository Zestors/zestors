use std::any::TypeId;

use crate::{all::*, Address};
use futures::future::BoxFuture;
pub use zestors_core::channel::*;
use zestors_core::config::Capacity;

pub trait ActorRefExt: ActorRef {
    fn get_address(&self) -> Address<Self::ActorKind> {
        let channel = <Self as ActorRef>::channel(&self).clone();
        channel.add_address();
        Address::from_channel(channel)
    }
    fn is_bounded(&self) -> bool {
        self.capacity().is_bounded()
    }
    fn close(&self) -> bool {
        <Self as ActorRef>::channel(self).close()
    }
    fn halt_some(&self, n: u32) {
        <Self as ActorRef>::channel(self).halt_some(n)
    }
    fn halt(&self) {
        <Self as ActorRef>::channel(self).halt()
    }
    fn process_count(&self) -> usize {
        <Self as ActorRef>::channel(self).process_count()
    }
    fn msg_count(&self) -> usize {
        <Self as ActorRef>::channel(self).msg_count()
    }
    fn address_count(&self) -> usize {
        <Self as ActorRef>::channel(self).address_count()
    }
    fn is_closed(&self) -> bool {
        <Self as ActorRef>::channel(self).is_closed()
    }
    fn capacity(&self) -> &Capacity {
        <Self as ActorRef>::channel(self).capacity()
    }
    fn has_exited(&self) -> bool {
        <Self as ActorRef>::channel(self).has_exited()
    }
    fn actor_id(&self) -> ActorId {
        <Self as ActorRef>::channel(self).actor_id()
    }
    fn try_send<M>(&self, msg: M) -> Result<M::Returned, TrySendError<M>>
    where
        M: Message,
        Self::ActorKind: Accept<M>,
    {
        <Self::ActorKind as Accept<M>>::try_send(<Self as ActorRef>::channel(self), msg)
    }
    fn send_now<M>(&self, msg: M) -> Result<M::Returned, TrySendError<M>>
    where
        M: Message,
        Self::ActorKind: Accept<M>,
    {
        <Self::ActorKind as Accept<M>>::send_now(<Self as ActorRef>::channel(self), msg)
    }
    fn send_blocking<M>(&self, msg: M) -> Result<M::Returned, SendError<M>>
    where
        M: Message,
        Self::ActorKind: Accept<M>,
    {
        <Self::ActorKind as Accept<M>>::send_blocking(<Self as ActorRef>::channel(self), msg)
    }
    fn send<M>(&self, msg: M) -> <Self::ActorKind as Accept<M>>::SendFut<'_>
    where
        M: Message,
        Self::ActorKind: Accept<M>,
    {
        <Self::ActorKind as Accept<M>>::send(<Self as ActorRef>::channel(self), msg)
    }
}
impl<T> ActorRefExt for T where T: ActorRef {}

/// A specializition of an [`ActorRef`] that allows for sending messages which are checked at runtime
/// whether the actor actually [accepts](Accept) the message.
pub trait DynActorRefExt: ActorRef
where
    Self::ActorKind: DynActorKind,
{
    fn try_send_unchecked<M>(&self, msg: M) -> Result<M::Returned, TrySendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
    {
        <Self as ActorRef>::channel(self).try_send_unchecked(msg)
    }
    fn send_now_unchecked<M>(&self, msg: M) -> Result<M::Returned, TrySendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
    {
        <Self as ActorRef>::channel(self).send_now_unchecked(msg)
    }
    fn send_blocking_unchecked<M>(&self, msg: M) -> Result<M::Returned, SendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
    {
        <Self as ActorRef>::channel(self).send_blocking_unchecked(msg)
    }
    fn send_unchecked<M>(&self, msg: M) -> BoxFuture<'_, Result<M::Returned, SendUncheckedError<M>>>
    where
        M::Returned: Send,
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
    {
        <Self as ActorRef>::channel(self).send_unchecked(msg)
    }
    fn accepts(&self, id: &TypeId) -> bool {
        <Self as ActorRef>::channel(self).accepts(id)
    }
}

impl<T> DynActorRefExt for T
where
    T: ActorRef,
    T::ActorKind: DynActorKind,
{
}
