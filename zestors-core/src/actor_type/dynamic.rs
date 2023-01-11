use crate::*;
use std::{any::TypeId, marker::PhantomData, sync::Arc};

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

pub trait TransformInto<T: DefinesDynChannel>: DefinesChannel {
    fn transform_into(channel: Arc<Self::Channel>) -> Arc<T::Channel>;
}

macro_rules! define_dynamic_protocol_types {
    ($($ident:ident $(<$( $ty:ident ),*>)?),*) => {$(
        /// One of the dynamic address-types, see [Dyn] for more documentation.
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
macro_rules! Accepts {
    () => {
        $crate::actor_type::Dyn<dyn $crate::actor_type::AcceptsNone>
    };
    ($ty1:ty) => {
        $crate::actor_type::Dyn<dyn $crate::actor_type::AcceptsOne<$ty1>>
    };
    ($ty1:ty, $ty2:ty) => {
        $crate::actor_type::Dyn<dyn $crate::actor_type::AcceptsTwo<$ty1, $ty2>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty) => {
        $crate::actor_type::Dyn<dyn $crate::actor_type::AcceptsThree<$ty1, $ty2, $ty3>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty) => {
        $crate::actor_type::Dyn<dyn $crate::actor_type::AcceptsFour<$ty1, $ty2, $ty3, $ty4>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty) => {
        $crate::actor_type::Dyn<dyn $crate::actor_type::AcceptsFive<$ty1, $ty2, $ty3, $ty4, $ty5>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty) => {
        $crate::actor_type::Dyn<dyn $crate::actor_type::AcceptsSix<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty) => {
        $crate::actor_type::Dyn<dyn $crate::actor_type::AcceptsSeven<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty) => {
        $crate::actor_type::Dyn<dyn $crate::actor_type::AcceptsEight<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty, $ty9:ty) => {
        $crate::actor_type::Dyn<dyn $crate::actor_type::AcceptsNine<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8, $ty9>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty, $ty9:ty, $ty10:ty) => {
        $crate::actor_type::Dyn<dyn $crate::actor_type::AcceptsTen<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8, $ty9, $ty10>>
    };
}

#[cfg(test)]
mod test {
    use crate::actor_type::{AcceptsOne, AcceptsTwo, Dyn};

    type _1 = Accepts![u32];
    type _2 = Dyn<dyn AcceptsOne<u32>>;
    type _3 = Accepts![u32, ()];
    type _4 = Dyn<dyn AcceptsTwo<u32, ()>>;
}
