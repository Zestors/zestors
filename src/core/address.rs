use std::{
    any::Any,
    io::{Read, Write},
};

use dyn_clone::DynClone;
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    core::*,
    distr::distr_addr::{Distr, RemoteMsg, DistrParam},
    Action,
};

//------------------------------------------------------------------------------------------------
//  Addressable
//------------------------------------------------------------------------------------------------

/// This trait allows for sending and calling any address type. This should be implemented for all
/// custom address types.
pub trait Addressable<A: 'static>: Address + Clone + Send {
    fn from_addr(addr: <Self::AddrType as AddrType>::Addr<A>) -> Self;
    fn as_addr(&self) -> &<Self::AddrType as AddrType>::Addr<A>;

    fn send<T>(&self, action: T) -> <Self::AddrType as AddrType>::SendResult<A>
    where
        T: Into<<Self::AddrType as AddrType>::Action<A>>,
    {
        self.as_addr().send(action)
    }

    fn call<M, MT, R>(
        &self,
        function: HandlerFn<A, Snd<MT>, R>,
        msg: M,
    ) -> <Self::AddrType as AddrType>::CallResult<MT, R>
    where
        M: Into<<Self::AddrType as AddrType>::Msg<MT>>,
        R: RcvPart,
        MT: Send + 'static,
    {
        self.as_addr().call(function, msg)
    }
}

pub type Param<'z, PT, AT> = <PT as ParamType<'z, AT>>::Param;
pub type Message<AT, M> = <AT as AddrType>::Msg<M>;
pub type CallResult<AT, M, R> = <AT as AddrType>::CallResult<M, R>;
//------------------------------------------------------------------------------------------------
//  Address
//------------------------------------------------------------------------------------------------

/// This trait must be implemented for all Addresses. It is used to convert boxed addresses back
/// into their original types, and also contains the address's location.
///
/// This trait therefore has to remain Unsized. The sized methods are kept in `Addressable`,
/// of which this is a subtrait.
pub trait Address: std::fmt::Debug + Send + DynClone {
    type AddrType: AddrType;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
    fn as_any(&self) -> &dyn Any;
    fn as_mut_any(&mut self) -> &mut dyn Any;
}

//------------------------------------------------------------------------------------------------
//  BoxedAddress
//------------------------------------------------------------------------------------------------

/// This trait allows for downcasting `Box<dyn Address>` to concrete types. It is automatically
/// implemented for anything that implements `Address`.
pub trait BoxedAddress: Sized {
    /// Attempt to downcast this address back into a fully typed address of type `T`.
    fn downcast<T: 'static + Address>(self) -> Result<T, Self>;

    /// Attempt to downcast this address back into a fully typed address of type `T`.
    fn downcast_ref<T: 'static + Address>(&self) -> Result<&T, ()>;

    /// Attempt to downcast this address back into a fully typed address of type `T`.
    fn downcast_mut<T: 'static + Address>(&mut self) -> Result<&mut T, ()>;

    /// Clone this address.
    fn clone(&self) -> Self;
}

impl<A: ?Sized + Address> BoxedAddress for Box<A> {
    fn downcast<T: 'static + Address>(self) -> Result<T, Self> {
        match self.as_any().downcast_ref::<T>() {
            Some(_) => Ok(*self.into_any().downcast().unwrap()),
            None => Err(self),
        }
    }

    fn downcast_ref<T: 'static + Address>(&self) -> Result<&T, ()> {
        self.as_any().downcast_ref::<T>().ok_or(())
    }

    fn downcast_mut<T: 'static + Address>(&mut self) -> Result<&mut T, ()> {
        self.as_mut_any().downcast_mut::<T>().ok_or(())
    }

    fn clone(&self) -> Self {
        dyn_clone::clone_box(self)
    }
}

//------------------------------------------------------------------------------------------------
//  AddrType
//------------------------------------------------------------------------------------------------

/// The location of an address. Can be either `Local`, `Remote` or `Anywhere`.
pub trait AddrType: 'static + Send {
    /// What address does this address use internally.
    type Addr<A: 'static>: Addressable<A, AddrType = Self>;

    /// What action can be sent to this address.
    type Action<A>;

    /// The result when calling this address.
    type CallResult<M, R: RcvPart>;

    /// The result when sending an action to this address.
    type SendResult<A>;

    /// The messages that can be sent to this address.
    /// This is just `M` for local addresses and `&'a M` for remote addresses.
    type Param<'a, P: 'a>
    where
        P: ParamType<'a, Self>;

    /// The message type which can be sent.
    /// This is LocalMsg<M> for local addresses and RemoteMsg<M> for remote addresses.
    type Msg<M>;
}

//------------------------------------------------------------------------------------------------
//  ParamType
//------------------------------------------------------------------------------------------------

/// Any value sent in a message must implement ParamType.
/// This trait just contains a single associated type `Param`.
/// This value is the value that is actually entered when sending a message to address of type AT
pub trait ParamType<'a, AT: AddrType>: Sized {
    type Param;
}

/// The ParamType `P` of a Local address is always just `P` whenver `P` is Send + 'static
impl<'a, P> ParamType<'a, Local> for P
where
    P: Send + 'static,
{
    type Param = P;
}

/// The ParamType `P` of a Remote address is `&P` whenever `P` is serializable
impl<'a, P> ParamType<'a, Distr> for P
where
    P: 'a + Serialize + DeserializeOwned + Clone + DistrParamType<'a>,
{
    type Param = &'a P;
}

//------------------------------------------------------------------------------------------------
//  RemoteParamType
//------------------------------------------------------------------------------------------------

/// For a type to be a distributed parameter, it must also implement the following methods:
pub trait DistrParamType<'a>: ParamType<'a, Distr> {
    /// The size this will have once serialized.
    fn serialized_size(param: &Self::Param) -> u64;
    /// Serializing the message into the buffer.
    fn serialize_into(param: Self::Param, buf: impl Write);
    /// Deserializing the paramter from the message
    fn deserialize_from(buf: impl Read) -> Self;
    fn into_local_param(param: Self::Param) -> Self;
}

impl<'a, P> DistrParamType<'a> for P
where
    P: 'a + Serialize + DeserializeOwned + Clone,
{
    fn serialize_into(param: Self::Param, buf: impl Write) {
        bincode::serialize_into(buf, param).unwrap()
    }

    fn deserialize_from(buf: impl Read) -> P {
        bincode::deserialize_from(buf).unwrap()
    }

    fn serialized_size(param: &Self::Param) -> u64 {
        bincode::serialized_size(*param).unwrap()
    }

    fn into_local_param(param: Self::Param) -> P {
        param.clone()
    }
}
