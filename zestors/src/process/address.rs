use crate::{
    channel::{ActorType, DynChannel},
    AcceptsAll, _gen,
};

use crate::*;
use futures::{Future, FutureExt};
use std::{any::TypeId, pin::Pin};
use tiny_actor::Channel;

/// # Types
/// An `Address` can be of two basic types: Dynamic or static.
///
/// ## Static
/// A static address is typed the [Protocol] of the actor: `Address<P>`. Any
/// messages the protocol accepts can be sent to this address.
///
/// ## Dynamic
/// A dynamic address is typed by the messages it accepts: [`DynAddress![u32, u64, ...]`](DynAddress!). Any messages
/// that appear can be sent to this address.
///
/// #### _For more information, please read the [module](crate) documentation._
pub struct Address<T: ActorType = AcceptsAll![]> {
    inner: tiny_actor::Address<T::Channel>,
}

impl<T: ActorType> Address<T> {
    pub(crate) fn from_inner(inner: tiny_actor::Address<T::Channel>) -> Self {
        Self { inner }
    }

    _gen::channel_methods!(inner);
    _gen::send_methods!(inner);
}

impl<P> Address<P>
where
    P: ActorType<Channel = Channel<P>>,
{
    _gen::into_dyn_methods!(inner, Address<T>);
}

impl<D> Address<D>
where
    D: ActorType<Channel = dyn DynChannel>,
{
    _gen::unchecked_send_methods!(inner);
    _gen::transform_methods!(inner, Address<T>);
}

impl<T: ActorType> Future for Address<T> {
    type Output = ();

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.inner.poll_unpin(cx)
    }
}

impl<T: ActorType> Unpin for Address<T> {}

impl<T: ActorType> Clone for Address<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: ActorType> std::fmt::Debug for Address<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Address").field(&self.inner).finish()
    }
}
