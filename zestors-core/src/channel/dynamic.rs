use crate::*;
use std::{any::TypeId, marker::PhantomData, sync::Arc};

/// Specifies a dynamic [Channel] which accepts certain messages:
/// - `Dyn<dyn AcceptsNone>` -> Accepts no messages
/// - `Dyn<dyn AcceptsTwo<u32, u64>` -> Accepts two messages:` u32` and `u64`.
///
/// In general, it is easier to use the [Accepts!] macro, for which the examples above become:
/// - `Accepts![]`
/// - `Accepts![u32, u64]`
pub struct Dyn<T: ?Sized>(PhantomData<*const T>);

unsafe impl<T: ?Sized> Send for Dyn<T> {}
unsafe impl<T: ?Sized> Sync for Dyn<T> {}

/// Indicates that a [channel definition](DefinesChannel) can transform into another one.
pub trait TransformInto<T: DefinesDynChannel>: DefinesChannel {
    fn transform_into(channel: Arc<Self::Channel>) -> Arc<T::Channel>;
}

macro_rules! define_dynamic_protocol_types {
    ($($ident:ident $(<$( $ty:ident ),*>)?),*) => {$(
        /// See [`Dyn`].
        pub trait $ident< $($($ty: Message,)?)*>: $($( ProtocolAccepts<$ty> + )?)* {}

        impl<$($($ty: Message + 'static,)?)*> DefinesDynChannel for Dyn<dyn $ident< $($($ty,)?)*>> {
            fn msg_ids() -> Box<[TypeId]> {
                Box::new([$($(TypeId::of::<$ty>(),)?)*])
            }
        }

        impl<D, $($($ty: Message + 'static,)?)*> TransformInto<Dyn<dyn $ident<$($($ty,)?)*>>> for Dyn<D>
        where
            Dyn<D>: DefinesDynChannel $($( + ProtocolAccepts<$ty> )?)*,
            D: ?Sized
        {
            fn transform_into(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel> {
                channel
            }
        }


        impl<P, $($($ty: Message + 'static,)?)*> TransformInto<Dyn<dyn $ident<$($($ty,)?)*>>> for P
        where
            P: Protocol $($( + ProtocolAccepts<$ty> )?)*
        {
            fn transform_into(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel> {
                channel
            }
        }
    )*};
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

/// Macro for writing dynamic [channel definitions](DefinesChannel):
/// - `Accepts![]` = `Dyn<dyn AcceptsNone>`
/// - `Accepts![u32, u64]` = `Dyn<dyn AcceptsTwo<u32, u64>`
///
/// These macros can be used as a generic argument to [children](Child) and [addresses](Address):
/// - `Address<Accepts![u32, String]>`
/// - `Child<_, Accepts![u32, String], _>`
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
    #[test]
    fn dynamic_definitions_compile() {
        type _1 = Accepts![()];
        type _2 = Accepts![(), ()];
        type _3 = Accepts![(), (), ()];
        type _4 = Accepts![(), (), (), ()];
        type _5 = Accepts![(), (), (), (), ()];
        type _6 = Accepts![(), (), (), (), (), ()];
        type _7 = Accepts![(), (), (), (), (), (), ()];
        type _8 = Accepts![(), (), (), (), (), (), (), ()];
        type _9 = Accepts![(), (), (), (), (), (), (), (), ()];
        type _10 = Accepts![(), (), (), (), (), (), (), (), (), ()];
    }
}
