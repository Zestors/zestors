use crate::*;
use futures::future::BoxFuture;
use std::any::TypeId;

pub trait ActorRef {
    type ChannelDefinition: DefineChannel;

    fn close(&self) -> bool;
    fn halt_some(&self, n: u32);
    fn halt(&self);
    fn process_count(&self) -> usize;
    fn msg_count(&self) -> usize;
    fn address_count(&self) -> usize;
    fn is_closed(&self) -> bool;
    fn is_bounded(&self) -> bool;
    fn capacity(&self) -> &Capacity;
    fn has_exited(&self) -> bool;
    fn actor_id(&self) -> ActorId;
    fn get_address(&self) -> Address<Self::ChannelDefinition>;

    fn try_send<M>(&self, msg: M) -> Result<Returned<M>, TrySendError<M>>
    where
        M: Message,
        Self::ChannelDefinition: Accept<M>;
    fn send_now<M>(&self, msg: M) -> Result<Returned<M>, TrySendError<M>>
    where
        M: Message,
        Self::ChannelDefinition: Accept<M>;
    fn send_blocking<M>(&self, msg: M) -> Result<Returned<M>, SendError<M>>
    where
        M: Message,
        Self::ChannelDefinition: Accept<M>;
    fn send<M>(&self, msg: M) -> <Self::ChannelDefinition as Accept<M>>::SendFut<'_>
    where
        M: Message,
        Self::ChannelDefinition: Accept<M>;
}

pub trait DynActorRef: ActorRef
where
    Self::ChannelDefinition: DefineDynChannel,
{
    fn try_send_unchecked<M>(&self, msg: M) -> Result<Returned<M>, TrySendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        Sent<M>: Send + 'static;
    fn send_now_unchecked<M>(&self, msg: M) -> Result<Returned<M>, TrySendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        Sent<M>: Send + 'static;
    fn send_blocking_unchecked<M>(&self, msg: M) -> Result<Returned<M>, SendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        Sent<M>: Send + 'static;
    fn send_unchecked<M>(
        &self,
        msg: M,
    ) -> BoxFuture<'_, Result<Returned<M>, SendUncheckedError<M>>>
    where
        M: Message + Send + 'static,
        <M::Type as MessageType<M>>::Returned: Send,
        Sent<M>: Send + 'static;
    fn accepts(&self, id: &TypeId) -> bool;
}
