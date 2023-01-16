use crate::*;
use std::{any::TypeId, marker::PhantomData, sync::Arc};

/// Indicates that an [ActorKind] can transform into another one.
pub trait TransformInto<T: ActorKind>: ActorKind {
    fn transform_into(channel: Arc<Self::Channel>) -> Arc<T::Channel>;
}

/// An [DynActorKind] with a dynamic [Channel] that accepts specific messages:
/// - `Dyn<dyn AcceptsNone>` -> Accepts no messages
/// - `Dyn<dyn AcceptsTwo<u32, u64>` -> Accepts two messages:` u32` and `u64`.
/// 
/// Any [InboxKind] which accepts these messages can be transformed into a dynamic channel using
/// [TransformInto].
pub struct Dyn<T: ?Sized>(PhantomData<*const T>);

unsafe impl<T: ?Sized> Send for Dyn<T> {}
unsafe impl<T: ?Sized> Sync for Dyn<T> {}

macro_rules! define_dynamic_protocol_types {
    ($($ident:ident $(<$( $ty:ident ),*>)?),*) => {$(
        /// A type `T` within [`Dyn<T>`].
        pub trait $ident< $($($ty: Message,)?)*>: $($( ProtocolFrom<$ty> + )?)* {}

        impl<$($($ty: Message + 'static,)?)*> DynActorKind for Dyn<dyn $ident< $($($ty,)?)*>> {
            fn msg_ids() -> Box<[TypeId]> {
                Box::new([$($(TypeId::of::<$ty>(),)?)*])
            }
        }

        impl<D, $($($ty: Message + 'static,)?)*> TransformInto<Dyn<dyn $ident<$($($ty,)?)*>>> for Dyn<D>
        where
            Dyn<D>: DynActorKind $($( + ProtocolFrom<$ty> )?)*,
            D: ?Sized
        {
            fn transform_into(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel> {
                channel
            }
        }

        impl<C, $($($ty: Message + 'static,)?)*> TransformInto<Dyn<dyn $ident<$($($ty,)?)*>>> for C
        where
            C: InboxKind $($( + Accept<$ty> )?)*,
            C::Channel: Sized
        {
            fn transform_into(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel> {
                <C as ActorKind>::into_dyn_channel(channel)
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
