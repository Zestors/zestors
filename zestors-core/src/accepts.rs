use std::marker::PhantomData;

use futures::Future;

use crate::*;

pub trait Accepts<M: Message>: AddressKind {
    fn try_send(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>>;
    fn send_now(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>>;
    fn send_blocking(address: &Self::Address, msg: M) -> Result<Returns<M>, SendError<M>>;
    fn send(address: &Self::Address, msg: M) -> SendFut<M>;
}

pub enum SendFut<M: Message> {
    Dynamic(M),
    Static(M),
}

impl<M> Future for SendFut<M>
where
    M: Message,
{
    type Output = Result<Returns<M>, SendError<M>>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        todo!()
    }
}

impl<M: Message, T> Accepts<M> for T
where
    T: AddressKind<Address = StaticAddress<T>> + ProtocolMessage<M>,
{
    fn try_send(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>> {
        address.try_send(msg)
    }

    fn send_now(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>> {
        todo!()
    }

    fn send_blocking(address: &Self::Address, msg: M) -> Result<Returns<M>, SendError<M>> {
        todo!()
    }

    fn send(address: &Self::Address, msg: M) -> SendFut<M> {
        todo!()
    }
}

pub trait AddressKind {
    type Address: tiny_actor::DynChannel;
}

impl<P: Protocol> AddressKind for P {
    type Address = StaticAddress<P>;
}

pub trait IntoDyn<D: ?Sized> {}

macro_rules! dyn_types {
    ($($ident:ident $(<$( $ty:ident ),*>)?),*) => {
        $(
            // Create the trait, which implements `DynAccepts`
            pub trait $ident< $($($ty: Message,)?)*>: $($( ProtocolMessage<$ty> + )?)* {}

            // Implement `IntoDyn<dyn Trait>` for any T which accepts all ty's
            impl<T, $($($ty: Message,)?)*> IntoDyn<Dyn<dyn $ident<$($($ty,)?)*>>> for Dyn<T>
            where
                T: ?Sized $($( + ProtocolMessage<$ty> )?)* {}
        )*
    };
}

pub struct Dyn<T: ?Sized>(PhantomData<*const T>);

impl<T: ?Sized> AddressKind for Dyn<T> {
    type Address = DynAddress<Dyn<T>>;
}

impl<M: Message, T: ?Sized> Accepts<M> for Dyn<T>
where
    Self: IntoDyn<Dyn<T>>,
{
    fn try_send(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>> {
        todo!()
    }

    fn send_now(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>> {
        todo!()
    }

    fn send_blocking(address: &Self::Address, msg: M) -> Result<Returns<M>, SendError<M>> {
        todo!()
    }

    fn send(address: &Self::Address, msg: M) -> SendFut<M> {
        todo!()
    }
}

dyn_types! {
    AcceptsNone,
    AcceptsOne<M1>,
    AcceptsTwo<M1, M2>,
    AcceptsThree<M1, M2, M3>,
    AcceptsFour<M1, M2, M3, M4>,
    AcceptsFive<M1, M2, M3, M4, M5>,
    AcceptsSix<M1, M2, M3, M4, M5, M6>,
    AcceptsSeven<M1, M2, M3, M4, M5, M6, M7>,
    AcceptsEight<M1, M2, M3, M4, M5, M6, M7, M8>,
    AcceptsNine<M1, M2, M3, M4, M5, M6, M7, M8, M9>,
    AcceptsTen<M1, M2, M3, M4, M5, M6, M7, M8, M9, M10>
}

#[macro_export]
macro_rules! Accepts {
    () => {
        dyn $crate::AcceptsNone
    };
    ($ty1:ty) => {
        dyn $crate::AcceptsOne<$ty1>
    };
    ($ty1:ty, $ty2:ty) => {
        dyn $crate::AcceptsTwo<$ty1, $ty2>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty) => {
        dyn $crate::AcceptsThree<$ty1, $ty2, $ty3>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty) => {
        dyn $crate::AcceptsFour<$ty1, $ty2, $ty3, $ty4>
    };
}

#[macro_export]
macro_rules! Address {
    ($($ty:ty),*) => {
        $crate::Address<Dyn<$crate::Accepts![$($ty),*]>>
    };
}
