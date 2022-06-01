use crate::core::*;
use dyn_clone::DynClone;
use std::any::Any;

//------------------------------------------------------------------------------------------------
//  Addressable
//------------------------------------------------------------------------------------------------

/// A trait implemented by all addresses.
///
/// It allows for sending `Action`s, or calling `HandlerFn`s.
pub trait Addressable<A: 'static>: Address + Clone {
    /// Create the address from the core address. (LocalAddr or DistrAddr)
    fn from_addr(addr: AddrT<Self::AddrType, A>) -> Self;

    /// Return a reference to the core address. (LocalAddr or DistrAddr)
    fn as_addr(&self) -> &AddrT<Self::AddrType, A>;

    /// Send an action to this address.
    fn send<T>(&self, action: T) -> SendResultT<Self::AddrType, A>
    where
        T: Into<ActionT<Self::AddrType, A>>,
    {
        self.as_addr().send(action)
    }

    /// Call a function on this address.
    fn call<M, MT, R>(
        &self,
        function: HandlerFn<A, MT, R>,
        msg: M,
    ) -> CallResultT<Self::AddrType, MT, R>
    where
        M: Into<MsgT<Self::AddrType, MT>>,
        MT: Send + 'static,
        R: RcvPart,
    {
        self.as_addr().call(function, msg)
    }
}

//------------------------------------------------------------------------------------------------
//  Address
//------------------------------------------------------------------------------------------------

/// A trait implemented by all addresses.
///
/// This trait is mainly used to make working with `Box<dyn AddressTrait>`s easier. This makes
/// it possible to do the following operations on boxed trait-objects: (also see `BoxedAddress`)
/// - Cloning
/// - Downcasting
///
/// This trait is a subtrait of `Addressable`, which allows for sending/calling an address.
pub trait Address: std::fmt::Debug + Send + DynClone {
    /// The type of this address (Local or Distr)
    type AddrType: AddrType;

    /// Get this address as a `Box<dyn Any>`.
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
    /// Get this address as a `&dyn Any`.
    fn as_any(&self) -> &dyn Any;
    /// Get this address as an `&mut dyn ANy`.
    fn as_mut_any(&mut self) -> &mut dyn Any;
}

//------------------------------------------------------------------------------------------------
//  BoxedAddress
//------------------------------------------------------------------------------------------------

/// This trait allows for downcasting `Box<dyn Address>` to concrete types, or cloning them.
///
/// It is automatically implemented for anything that implements `Address`.
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

/// Implement BoxedAddress for any Box<impl Address>.
impl<A> BoxedAddress for Box<A>
where
    A: ?Sized + Address,
{
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
