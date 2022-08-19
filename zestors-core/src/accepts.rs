use crate::*;
use std::{any::TypeId, marker::PhantomData};

pub trait Accepts<M: Message>: AddressKind {
    fn try_send(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>>;
    fn send_now(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>>;
    fn send_blocking(address: &Self::Address, msg: M) -> Result<Returns<M>, SendError<M>>;
    fn send(address: &Self::Address, msg: M) -> SendFut<M>;
}

pub trait AddressKind {
    type Address: tiny_actor::DynChannel + Clone;
}

impl<P: Protocol> AddressKind for P {
    type Address = StaticAddress<P>;
}

pub trait IntoDyn<D: ?Sized> {}

pub struct Dyn<T: ?Sized>(PhantomData<T>);

unsafe impl<T: ?Sized> Send for Dyn<T> {}
unsafe impl<T: ?Sized> Sync for Dyn<T> {}

impl<T: ?Sized> AddressKind for Dyn<T> {
    type Address = DynAddress<Dyn<T>>;
}

pub trait IsDyn {
    fn message_ids() -> Box<[TypeId]>;
}

macro_rules! dyn_types {
    ($($ident:ident $(<$( $ty:ident ),*>)?),*) => {
        $(
            // Create the trait, which implements `DynAccepts`
            pub trait $ident< $($($ty: Message,)?)*>: $($( ProtocolMessage<$ty> + )?)* {}

            // Implement `IntoDyn<dyn Trait>` for any T which accepts all ty's
            impl<T, $($($ty: Message,)?)*> IntoDyn<Dyn<dyn $ident<$($($ty,)?)*>>> for Dyn<T>
            where
                T: ?Sized $($( + ProtocolMessage<$ty> )?)* {}

            impl<$($($ty: Message + 'static,)?)*> IsDyn for Dyn<dyn $ident< $($($ty,)?)*>> {
                fn message_ids() -> Box<[TypeId]> {
                    Box::new([$($(TypeId::of::<$ty>(),)?)*])
                }
            }
        )*
    };
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
        Dyn<dyn $crate::AcceptsNone>
    };
    ($ty1:ty) => {
        Dyn<dyn $crate::AcceptsOne<$ty1>>
    };
    ($ty1:ty, $ty2:ty) => {
        Dyn<dyn $crate::AcceptsTwo<$ty1, $ty2>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty) => {
        Dyn<dyn $crate::AcceptsThree<$ty1, $ty2, $ty3>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty) => {
        Dyn<dyn $crate::AcceptsFour<$ty1, $ty2, $ty3, $ty4>>
    };
}

#[macro_export]
macro_rules! Address {
    ($($ty:ty),*) => {
        $crate::Address<$crate::Accepts![$($ty),*]>
    };
}
