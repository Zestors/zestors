use super::*;
use std::{any::TypeId, marker::PhantomData};

/// Signifies an actor that is dynamically typed by the messages that it accepts,
/// for example:
/// - `Address<Dyn<dyn AcceptsNone>>`
/// - `Address<Dyn<dyn AcceptsTwo<u32, u64>>`
///
/// For most uses it is easier to use the [AcceptsAll!] macro, for which the
/// examples above become:
/// - `Address<AcceptsAll![]>`
/// - `Address<AcceptsAll![u32, u64]>`
///
/// This means that `Dyn<dyn AcceptsNone>` == `AcceptsAll![]`, and they can be used
/// interchangably.
pub struct Dyn<T: ?Sized>(PhantomData<*const T>);

unsafe impl<T: ?Sized> Send for Dyn<T> {}
unsafe impl<T: ?Sized> Sync for Dyn<T> {}

/// Trait implemented for all [`Dyn<...>`](Dyn) types, this never has to be implemented
/// manually.
pub trait IsDyn {
    fn msg_ids() -> Box<[TypeId]>;
}

/// Marker trait which is implemented for any [`Dyn<...>`](Dyn) or [Protocol], that can
/// be converted into `D`. This never has to be implemented manually.
pub trait IntoDyn<D> {}

macro_rules! define_dynamic_protocol_types {
    ($($ident:ident $(<$( $ty:ident ),*>)?),*) => {
        $(
            /// One of the dynamic address-types, see [Dyn] for more documentation.
            pub trait $ident< $($($ty: Message,)?)*>: $($( ProtocolMessage<$ty> + )?)* {}

            impl<$($($ty: Message + 'static,)?)*> IsDyn for Dyn<dyn $ident< $($($ty,)?)*>> {
                fn msg_ids() -> Box<[TypeId]> {
                    Box::new([$($(TypeId::of::<$ty>(),)?)*])
                }
            }

            impl<D, $($($ty: Message + 'static,)?)*> IntoDyn<Dyn<dyn $ident<$($($ty,)?)*>>> for Dyn<D>
            where
                D: IsDyn + ?Sized $($( + ProtocolMessage<$ty> )?)* {}

            impl<P, $($($ty: Message + 'static,)?)*> IntoDyn<Dyn<dyn $ident<$($($ty,)?)*>>> for P
            where
                P: Protocol $($( + ProtocolMessage<$ty> )?)* {}
        )*
    };
}

define_dynamic_protocol_types! {
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

/// This macro makes it easier to write [`dynamic protocol types`](Dyn). Instead of writing for
/// example:
/// - `Address<Dyn<dyn AcceptsNone>>`
/// - `Address<Dyn<dyn AcceptsTwo<u32, u64>>`
///
/// This can now be written as:
/// - `Address<AcceptsAll![]>`
/// - `Address<AcceptsAll![u32, u64]>`
///
/// This means that `Dyn<dyn AcceptsNone>` == `AcceptsAll![]`, and they can be used
/// interchangably.
#[macro_export]
macro_rules! AcceptsAll {
    () => {
        $crate::protocol::Dyn<dyn $crate::protocol::AcceptsNone>
    };
    ($ty1:ty) => {
        $crate::protocol::Dyn<dyn $crate::protocol::AcceptsOne<$ty1>>
    };
    ($ty1:ty, $ty2:ty) => {
        $crate::protocol::Dyn<dyn $crate::protocol::AcceptsTwo<$ty1, $ty2>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty) => {
        $crate::protocol::Dyn<dyn $crate::protocol::AcceptsThree<$ty1, $ty2, $ty3>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty) => {
        $crate::protocol::Dyn<dyn $crate::protocol::AcceptsFour<$ty1, $ty2, $ty3, $ty4>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty) => {
        $crate::protocol::Dyn<dyn $crate::protocol::AcceptsFive<$ty1, $ty2, $ty3, $ty4, $ty5>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty) => {
        $crate::protocol::Dyn<dyn $crate::protocol::AcceptsSix<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty) => {
        $crate::protocol::Dyn<dyn $crate::protocol::AcceptsSeven<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty) => {
        $crate::protocol::Dyn<dyn $crate::protocol::AcceptsEight<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty, $ty9:ty) => {
        $crate::protocol::Dyn<dyn $crate::protocol::AcceptsNine<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8, $ty9>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty, $ty9:ty, $ty10:ty) => {
        $crate::protocol::Dyn<dyn $crate::protocol::AcceptsTen<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8, $ty9, $ty10>>
    };
}

mod test {
    use crate::protocol::{AcceptsOne, AcceptsTwo, Dyn};

    type _X = AcceptsAll![u32];
    type _XX = Dyn<dyn AcceptsOne<u32>>;
    type _Y = AcceptsAll![u32, ()];
    type _YY = Dyn<dyn AcceptsTwo<u32, ()>>;
}
