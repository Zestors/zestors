use crate::*;
use std::{any::TypeId, marker::PhantomData};

//------------------------------------------------------------------------------------------------
//  AddressType
//------------------------------------------------------------------------------------------------

/// An `AddressType` signifies the type that an address can be. The type of an address can be either
/// a [Protocol] or a [Dyn<_>] type.
pub trait AddressType {
    type Address: tiny_actor::DynChannel + Clone;
}

impl<T: Protocol> AddressType for T {
    type Address = StaticAddress<T>;
}

impl<T: ?Sized> AddressType for Dyn<T> {
    type Address = DynAddress<Dyn<T>>;
}

//------------------------------------------------------------------------------------------------
//  Dyn
//------------------------------------------------------------------------------------------------

/// The dynamic [AddressType].
pub struct Dyn<D: ?Sized>(PhantomData<*const D>);

unsafe impl<D: ?Sized> Send for Dyn<D> {}
unsafe impl<D: ?Sized> Sync for Dyn<D> {}

//------------------------------------------------------------------------------------------------
//  Accepts
//------------------------------------------------------------------------------------------------

/// Whether an actor accepts messages of a certain kind. If this is implemented for the
/// [AddressType] then messages of type `M` can be sent to it's address.
pub trait Accepts<M: Message>: AddressType {
    fn try_send(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>>;
    fn send_now(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>>;
    fn send_blocking(address: &Self::Address, msg: M) -> Result<Returns<M>, SendError<M>>;
    fn send(address: &Self::Address, msg: M) -> SendFut<M>;
}

//------------------------------------------------------------------------------------------------
//  IntoDyn
//------------------------------------------------------------------------------------------------

/// Marker trait that signifies whether an address can be converted to a dynamic [AddressType] `T`.
pub trait IntoDyn<T> {}

//------------------------------------------------------------------------------------------------
//  IsDyn
//------------------------------------------------------------------------------------------------

/// Trait implemented for all dynamic [AddressType]s.
pub trait IsDyn {
    /// Get all message-ids that are accepted by this [AddressType].
    fn message_ids() -> Box<[TypeId]>;
}

//------------------------------------------------------------------------------------------------
//  Dynamic types
//------------------------------------------------------------------------------------------------

macro_rules! dyn_types {
    ($($ident:ident $(<$( $ty:ident ),*>)?),*) => {
        $(
            // Create the trait

            /// A dynamic address-type.
            pub trait $ident< $($($ty: Message,)?)*>: $($( ProtocolMessage<$ty> + )?)* {}

            // Implement `IsDyn` for it
            impl<$($($ty: Message + 'static,)?)*> IsDyn for Dyn<dyn $ident< $($($ty,)?)*>> {
                fn message_ids() -> Box<[TypeId]> {
                    Box::new([$($(TypeId::of::<$ty>(),)?)*])
                }
            }

            // Implement `IntoDyn` for all dynamic address-types
            impl<T, $($($ty: Message,)?)*> IntoDyn<Dyn<dyn $ident<$($($ty,)?)*>>> for Dyn<T>
            where
                T: ?Sized $($( + ProtocolMessage<$ty> )?)* {}

            // Implement `IntoDyn` for all static address-types
            impl<T, $($($ty: Message,)?)*> IntoDyn<Dyn<dyn $ident<$($($ty,)?)*>>> for T
            where
                T: Protocol $($( + ProtocolMessage<$ty> )?)* {}
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

//------------------------------------------------------------------------------------------------
//  Address
//------------------------------------------------------------------------------------------------

/// A macro to easily create dynamic [Address]es.
///
/// See [Accepts!] for creating dynamic [AddressType]s.
///
/// # Examples
/// * `Address![]` == `Address<Dyn<dyn AcceptsNone>>`
/// * `Address![u32, u64]` == `Address<Dyn<dyn AcceptsTwo<u32, u64>>>`
#[macro_export]
macro_rules! Address {
    ($($ty:ty),*) => {
        $crate::Address<$crate::Accepts![$($ty),*]>
    };
}

//------------------------------------------------------------------------------------------------
//  Accepts
//------------------------------------------------------------------------------------------------

/// A macro to easily create dynamic [AddressType]s.
///
/// See [Address!] for creating dynamic [Address]es.
///
/// # Examples
/// * `Accepts![u32, u64]` == `Dyn<dyn AcceptsTwo<u32, u64>>`
/// * `Address<Accepts![]>` == `Address<Dyn<dyn AcceptsNone>>`
/// * `Address<Accepts![u32, u64]>` == `Address<Dyn<dyn AcceptsTwo<u32, u64>>>`
#[macro_export]
macro_rules! Accepts {
    () => {
        $crate::Dyn<dyn $crate::AcceptsNone>
    };
    ($ty1:ty) => {
        $crate::Dyn<dyn $crate::AcceptsOne<$ty1>>
    };
    ($ty1:ty, $ty2:ty) => {
        $crate::Dyn<dyn $crate::AcceptsTwo<$ty1, $ty2>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty) => {
        $crate::Dyn<dyn $crate::AcceptsThree<$ty1, $ty2, $ty3>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty) => {
        $crate::Dyn<dyn $crate::AcceptsFour<$ty1, $ty2, $ty3, $ty4>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty) => {
        $crate::Dyn<dyn $crate::AcceptsFive<$ty1, $ty2, $ty3, $ty4, $ty5>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty) => {
        $crate::Dyn<dyn $crate::AcceptsSix<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty) => {
        $crate::Dyn<dyn $crate::AcceptsSeven<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty) => {
        $crate::Dyn<dyn $crate::AcceptsEight<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty, $ty9:ty) => {
        $crate::Dyn<dyn $crate::AcceptsNine<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8, $ty9>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty, $ty9:ty, $ty10:ty) => {
        $crate::Dyn<dyn $crate::AcceptsTen<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8, $ty9, $ty10>>
    };
}

#[cfg(test)]
mod test {
    use std::any::TypeId;

    use crate::IsDyn;

    #[test]
    fn address_macro_compiles() {
        let a: Address![];
        let a: Address![u8];
        let a: Address![u8, u16];
        let a: Address![u8, u16, u32];
        let a: Address![u8, u16, u32, u64];
        let a: Address![u8, u16, u32, u64, u128];
        let a: Address![u8, u16, u32, u64, u128, i8];
        let a: Address![u8, u16, u32, u64, u128, i8, i16];
        let a: Address![u8, u16, u32, u64, u128, i8, i16, i32];
        let a: Address![u8, u16, u32, u64, u128, i8, i16, i32, i64];
        let a: Address![u8, u16, u32, u64, u128, i8, i16, i32, i64, i128];
    }

    #[test]
    fn message_ids() {
        assert_eq!(
            <Accepts![] as IsDyn>::message_ids(),
            Box::new([]) as Box<[TypeId]>
        );

        assert_eq!(
            <Accepts![u32, u64] as IsDyn>::message_ids(),
            Box::new([TypeId::of::<u32>(), TypeId::of::<u64>()]) as Box<[TypeId]>
        );

        assert_eq!(
            <Accepts![u32, u64, i8] as IsDyn>::message_ids(),
            Box::new([TypeId::of::<u32>(), TypeId::of::<u64>(), TypeId::of::<i8>()])
                as Box<[TypeId]>
        );
    }
}
