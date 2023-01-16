use crate::*;
use futures::future::BoxFuture;
use std::{any::TypeId, sync::Arc};



pub trait ActorRef {
    type ActorKind: ActorKind;
    
    fn close(&self) -> bool;
    fn halt_some(&self, n: u32);
    fn halt(&self);
    fn process_count(&self) -> usize;
    fn msg_count(&self) -> usize;
    fn address_count(&self) -> usize;
    fn is_closed(&self) -> bool;
    fn capacity(&self) -> &Capacity;
    fn has_exited(&self) -> bool;
    fn actor_id(&self) -> ActorId;
    fn channel(actor_ref: &Self) -> &Arc<<Self::ActorKind as ActorKind>::Channel>;
    fn try_send<M>(&self, msg: M) -> Result<M::Returned, TrySendError<M>>
    where
        M: Message,
        Self::ActorKind: Accept<M>;
    fn send_now<M>(&self, msg: M) -> Result<M::Returned, TrySendError<M>>
    where
        M: Message,
        Self::ActorKind: Accept<M>;
    fn send_blocking<M>(&self, msg: M) -> Result<M::Returned, SendError<M>>
    where
        M: Message,
        Self::ActorKind: Accept<M>;
    fn send<M>(&self, msg: M) -> <Self::ActorKind as Accept<M>>::SendFut<'_>
    where
        M: Message,
        Self::ActorKind: Accept<M>;
}

pub trait DynActorRef: ActorRef
where
    Self::ActorKind: DynActorKind,
{
    fn try_send_unchecked<M>(&self, msg: M) -> Result<M::Returned, TrySendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        M::Payload: Send + 'static;
    fn send_now_unchecked<M>(&self, msg: M) -> Result<M::Returned, TrySendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        M::Payload: Send + 'static;
    fn send_blocking_unchecked<M>(&self, msg: M) -> Result<M::Returned, SendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        M::Payload: Send + 'static;
    fn send_unchecked<M>(
        &self,
        msg: M,
    ) -> BoxFuture<'_, Result<M::Returned, SendUncheckedError<M>>>
    where
        M: Message + Send + 'static,
        M::Returned: Send,
        M::Payload: Send + 'static;
    fn accepts(&self, id: &TypeId) -> bool;
}
