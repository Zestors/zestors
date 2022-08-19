use crate::*;
use futures::Future;
use std::pin::Pin;

mod dyn_address;
mod static_address;

pub use dyn_address::*;
pub use static_address::*;

pub struct Address<T: AddressKind> {
    address: T::Address,
}

impl<T: AddressKind> Clone for Address<T> {
    fn clone(&self) -> Self {
        Self {
            address: self.address.clone(),
        }
    }
}

impl<T: AddressKind> Address<T> {
    fn close(&self) -> bool {
        self.address.close()
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
    P: AddressKind<Address = StaticAddress<P>>,
{
    pub fn into_dyn<D>(self) -> Address<D>
    where
        P: Protocol + IntoDyn<D>,
        D: AddressKind<Address = DynAddress<D>>,
    {
        Address {
            address: self.address.into_dyn(),
        }
    }
}

impl<D> Address<D>
where
    D: AddressKind<Address = DynAddress<D>>,
{
    pub fn transform_unchecked<T>(self) -> Address<T>
    where
        T: AddressKind<Address = DynAddress<T>>,
    {
        Address {
            address: self.address.transform_unchecked(),
        }
    }

    pub fn transform<T>(self) -> Address<T>
    where
        T: AddressKind<Address = DynAddress<T>>,
        D: IntoDyn<T>,
    {
        Address {
            address: self.address.transform::<T>(),
        }
    }

    pub fn try_transform<T>(self) -> Result<Address<T>, Self>
    where
        T: AddressKind<Address = DynAddress<T>> + IsDyn,
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

pub type SendFut<'a, M> =
    Pin<Box<dyn Future<Output = Result<Returns<M>, SendError<M>>> + Send + 'a>>;
