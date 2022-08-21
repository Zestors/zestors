use crate::*;

use futures::Future;
use std::pin::Pin;

mod dyn_address;
mod static_address;

pub use dyn_address::*;
pub use static_address::*;

pub struct Address<T: AddressType> {
    address: T::Address,
}

impl<T: AddressType> Address<T> {
    pub fn close(&self) -> bool {
        self.address.close()
    }
    pub fn halt_some(&self, n: u32) {
        self.address.halt_some(n)
    }
    pub fn process_count(&self) -> usize {
        self.address.inbox_count()
    }
    pub fn msg_count(&self) -> usize {
        self.address.msg_count()
    }
    pub fn address_count(&self) -> usize {
        self.address.address_count()
    }
    pub fn is_closed(&self) -> bool {
        self.address.is_closed()
    }
    pub fn capacity(&self) -> &Capacity {
        self.address.capacity()
    }
    pub fn has_exited(&self) -> bool {
        self.address.has_exited()
    }
    pub fn actor_id(&self) -> u64 {
        self.address.actor_id()
    }
    pub fn is_bounded(&self) -> bool {
        self.address.is_bounded()
    }

    pub fn try_send<M>(&self, msg: M) -> Result<Returns<M>, TrySendError<M>>
    where
        M: Message,
        T: Accepts<M>,
    {
        <T as Accepts<M>>::try_send(&self.address, msg)
    }
    pub fn send_now<M>(&self, msg: M) -> Result<Returns<M>, TrySendError<M>>
    where
        M: Message,
        T: Accepts<M>,
    {
        <T as Accepts<M>>::send_now(&self.address, msg)
    }
    pub fn send_blocking<M>(&self, msg: M) -> Result<Returns<M>, SendError<M>>
    where
        M: Message,
        T: Accepts<M>,
    {
        <T as Accepts<M>>::send_blocking(&self.address, msg)
    }
    pub async fn send<M>(&self, msg: M) -> Result<Returns<M>, SendError<M>>
    where
        M: Message,
        T: Accepts<M>,
    {
        <T as Accepts<M>>::send(&self.address, msg).await
    }
}

impl<P> Address<P>
where
    P: AddressType<Address = StaticAddress<P>>,
{
    pub fn into_dyn<D>(self) -> Address<D>
    where
        P: Protocol + IntoDyn<D>,
        D: AddressType<Address = DynAddress<D>>,
    {
        Address {
            address: self.address.into_dyn(),
        }
    }
}

impl<D> Address<D>
where
    D: AddressType<Address = DynAddress<D>>,
{
    pub fn transform_unchecked<T>(self) -> Address<T>
    where
        T: AddressType<Address = DynAddress<T>>,
    {
        Address {
            address: self.address.transform_unchecked(),
        }
    }
    pub fn transform<T>(self) -> Address<T>
    where
        T: AddressType<Address = DynAddress<T>>,
        D: IntoDyn<T>,
    {
        Address {
            address: self.address.transform::<T>(),
        }
    }
    pub fn try_transform<T>(self) -> Result<Address<T>, Self>
    where
        T: AddressType<Address = DynAddress<T>> + IsDyn,
        D: IntoDyn<T>,
    {
        match self.address.try_transform() {
            Ok(address) => Ok(Address { address }),
            Err(address) => Err(Address { address }),
        }
    }
    pub fn downcast<P: Protocol>(self) -> Result<Address<P>, Self> {
        match self.address.downcast::<P>() {
            Ok(address) => Ok(Address { address }),
            Err(address) => Err(Address { address }),
        }
    }

    pub fn try_send_unchecked<M>(&self, msg: M) -> Result<Returns<M>, TrySendDynError<M>>
    where
        M: Message + Send + 'static,
        Sends<M>: Send + 'static,
    {
        self.address.try_send_unchecked(msg)
    }
    pub fn send_now_unchecked<M>(&self, msg: M) -> Result<Returns<M>, TrySendDynError<M>>
    where
        M: Message + Send + 'static,
        Sends<M>: Send + 'static,
    {
        self.address.send_now_unchecked(msg)
    }
    pub fn send_blocking_unchecked<M>(&self, msg: M) -> Result<Returns<M>, SendDynError<M>>
    where
        M: Message + Send + 'static,
        Sends<M>: Send + 'static,
    {
        self.address.send_blocking_unchecked(msg)
    }
    pub async fn send_unchecked<M>(&self, msg: M) -> Result<Returns<M>, SendDynError<M>>
    where
        M: Message + Send + 'static,
        Sends<M>: Send + 'static,
    {
        self.address.send_unchecked(msg).await
    }
}

impl<T: AddressType> Clone for Address<T> {
    fn clone(&self) -> Self {
        Self {
            address: self.address.clone(),
        }
    }
}

pub type SendFut<'a, M> =
    Pin<Box<dyn Future<Output = Result<Returns<M>, SendError<M>>> + Send + 'a>>;

//------------------------------------------------------------------------------------------------
//  IntoAddress
//------------------------------------------------------------------------------------------------

pub trait IntoAddress<T: AddressType> {
    fn into_address(self) -> Address<T>;
}

impl<D: ?Sized, T: ?Sized> IntoAddress<Dyn<T>> for Address<Dyn<D>>
where
    Dyn<D>: AddressType<Address = DynAddress<Dyn<D>>> + IntoDyn<Dyn<T>>,
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
