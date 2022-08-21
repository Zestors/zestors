use crate::*;
use futures::{Future, FutureExt};
use std::{any::TypeId, pin::Pin};

//------------------------------------------------------------------------------------------------
//  Address
//------------------------------------------------------------------------------------------------

/// # Types
/// An `Address` can be of two basic types: Dynamic or static.
///
/// ## Static
/// A static address is typed the [Protocol] of the actor: `Address<P> where P: Protocol`. Any
/// messages the protocol accepts can be sent to this address.
///
/// ## Dynamic
/// A dynamic address is typed by the messages it accepts: [`Address![u32, u64, ...]`](Address!). Any messages
/// that appear can be sent to this address. An `Address![]` can also be written as an `Address`.
///
/// __Note__: `Address![u32, u64]` == `Address<Accepts![u32, u64]>` ==
/// `Address<Dyn<dyn AcceptsTwo<u32, u64>>`, it does not matter which of these representations is used.
/// 
/// # Awaiting
/// An `Address` can be awaited and will return once the actor has exited.
/// 
/// #### _For more information, please read the [module](crate) documentation._
pub struct Addr<T: ActorType = DynAccepts![]> {
    inner: tiny_actor::Address<<T::Type as ChannelType>::Channel>,
}

//-------------------------------------------------
//  Implementation
//-------------------------------------------------

impl<T: ActorType> Addr<T> {
    pub fn from_inner(inner: tiny_actor::Address<<T::Type as ChannelType>::Channel>) -> Self {
        Self { inner }
    }

    gen::channel_methods!(inner);
    gen::send_methods!(inner);
}

impl<P> Addr<P>
where
    P: ActorType<Type = Static<P>>,
{
    gen::into_dyn_methods!(inner, Addr<T>);
}

impl<D> Addr<D>
where
    D: ActorType<Type = Dynamic>,
{
    gen::unchecked_send_methods!(inner);
    gen::transform_methods!(inner, Addr<T>);
}

//-------------------------------------------------
//  Traits
//-------------------------------------------------

impl<T: ActorType> Future for Addr<T> {
    type Output = ();

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.inner.poll_unpin(cx)
    }
}

impl<T: ActorType> Unpin for Addr<T> {}

impl<T: ActorType> Clone for Addr<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: ActorType> std::fmt::Debug for Addr<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Address").field(&self.inner).finish()
    }
}
