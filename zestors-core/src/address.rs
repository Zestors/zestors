use crate::*;
use futures::Future;
use std::{any::TypeId, pin::Pin};

//------------------------------------------------------------------------------------------------
//  Address
//------------------------------------------------------------------------------------------------

pub struct Address<T: ActorType = Accepts![]> {
    address: tiny_actor::Address<<T::Type as ChannelType>::Channel>,
}

//-------------------------------------------------
//  Any address
//-------------------------------------------------

impl<T: ActorType> Address<T> {
    pub fn from_inner(address: tiny_actor::Address<<T::Type as ChannelType>::Channel>) -> Self {
        Self { address }
    }

    gen::channel_methods!(address);
    gen::send_methods!(address);
}

//-------------------------------------------------
//  Static address
//-------------------------------------------------

impl<P> Address<P>
where
    P: ActorType<Type = Static<P>>,
{
    pub fn into_dyn<D>(self) -> Address<D>
    where
        P: Protocol + IntoDyn<D>,
        D: ActorType<Type = Dynamic>,
    {
        Address {
            address: self.address.transform_channel(|c| c),
        }
    }
}

//-------------------------------------------------
//  Dynamic address
//-------------------------------------------------

impl<D> Address<D>
where
    D: ActorType<Type = Dynamic>,
{
    gen::unchecked_send_methods!(address);

    pub fn transform_unchecked<T>(self) -> Address<T>
    where
        T: ActorType<Type = Dynamic>,
    {
        Address {
            address: self.address,
        }
    }

    pub fn transform<T>(self) -> Address<T>
    where
        D: IntoDyn<T>,
        T: ActorType<Type = Dynamic>,
    {
        self.transform_unchecked()
    }

    pub fn try_transform<T>(self) -> Result<Address<T>, Self>
    where
        T: IsDyn,
        T: ActorType<Type = Dynamic>,
    {
        if T::message_ids().iter().all(|id| self.accepts(id)) {
            Ok(self.transform_unchecked())
        } else {
            Err(self)
        }
    }

    pub fn accepts(&self, id: &TypeId) -> bool {
        self.address.channel_ref().accepts(id)
    }

    pub fn downcast<P>(self) -> Result<Address<P>, Self>
    where
        P: Protocol,
    {
        match self.address.downcast::<P>() {
            Ok(address) => Ok(Address { address }),
            Err(address) => Err(Self { address }),
        }
    }
}

//-------------------------------------------------
//  Address traits
//-------------------------------------------------

impl<T: ActorType> Clone for Address<T> {
    fn clone(&self) -> Self {
        Self {
            address: self.address.clone(),
        }
    }
}

impl<T: ActorType> std::fmt::Debug for Address<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Address").field(&self.address).finish()
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
    Dyn<R>: ActorType<Type = Dynamic> + IntoDyn<Dyn<T>>,
{
    fn into_address(self) -> Address<Dyn<T>> {
        self.transform()
    }
}

impl<P, T> IntoAddress<Dyn<T>> for Address<P>
where
    P: Protocol + IntoDyn<Dyn<T>>,
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
