use crate::*;
use futures::{Future, FutureExt};
use std::{any::TypeId, pin::Pin};

//------------------------------------------------------------------------------------------------
//  Address
//------------------------------------------------------------------------------------------------

pub struct Address<T: ActorType = Accepts![]> {
    inner: tiny_actor::Address<<T::Type as ChannelType>::Channel>,
}

//-------------------------------------------------
//  Any address
//-------------------------------------------------

impl<T: ActorType> Address<T> {
    pub fn from_inner(inner: tiny_actor::Address<<T::Type as ChannelType>::Channel>) -> Self {
        Self { inner }
    }

    gen::channel_methods!(inner);
    gen::send_methods!(inner);
}

//-------------------------------------------------
//  Static address
//-------------------------------------------------

impl<P> Address<P>
where
    P: ActorType<Type = Static<P>>,
{
    gen::into_dyn_methods!(inner, Address<T>);
}

//-------------------------------------------------
//  Dynamic address
//-------------------------------------------------

impl<D> Address<D>
where
    D: ActorType<Type = Dynamic>,
{
    gen::unchecked_send_methods!(inner);
    gen::transform_methods!(inner, Address<T>);
}

//-------------------------------------------------
//  Address traits
//-------------------------------------------------

impl<T: ActorType> Unpin for Address<T> {}

impl<T: ActorType> Future for Address<T> {
    type Output = ();

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.inner.poll_unpin(cx)
    }
}

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

//------------------------------------------------------------------------------------------------
//  IntoAddress
//------------------------------------------------------------------------------------------------

pub trait IntoAddress<T: ActorType> {
    fn into_address(self) -> Address<T>;
}

impl<R: ?Sized, T: ?Sized> IntoAddress<Dyn<T>> for Address<Dyn<R>>
where
    Dyn<R>: ActorType<Type = Dynamic> + IntoDynamic<Dyn<T>>,
{
    fn into_address(self) -> Address<Dyn<T>> {
        self.transform()
    }
}

impl<P, T> IntoAddress<Dyn<T>> for Address<P>
where
    P: Protocol + IntoDynamic<Dyn<T>>,
    T: ?Sized,
{
    fn into_address(self) -> Address<Dyn<T>> {
        self.into_dyn()
    }
}

//------------------------------------------------------------------------------------------------
//  SendFut
//------------------------------------------------------------------------------------------------

pub type SendFut<'a, M> =
    Pin<Box<dyn Future<Output = Result<Returns<M>, SendError<M>>> + Send + 'a>>;
