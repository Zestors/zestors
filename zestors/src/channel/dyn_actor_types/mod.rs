/// Module containing all dynamic [actor types](ActorType).
use crate::all::*;
use std::{any::TypeId, sync::Arc};

macro_rules! create_dynamic_actor_types {
    ($($ident:ident $(<$( $ty:ident ),*>)?),*) => {$(
        /// See [`Dyn`].
        pub trait $ident< $($($ty: Message,)?)*>: std::fmt::Debug + $($( ProtocolFrom<$ty> + )?)* {}

        impl<$($($ty: Message + 'static,)?)*> DynActorType for Dyn<dyn $ident< $($($ty,)?)*>> {
            fn msg_ids() -> Box<[TypeId]> {
                Box::new([$($(TypeId::of::<$ty>(),)?)*])
            }
        }

        impl<D, $($($ty: Message + 'static,)?)*> TransformInto<Dyn<dyn $ident<$($($ty,)?)*>>> for Dyn<D>
        where
            Dyn<D>: DynActorType,
            D: ?Sized $($( + ProtocolFrom<$ty> )?)*
        {
            fn transform_into(channel: Arc<Self::Channel>) -> Arc<dyn Channel> {
                channel
            }
        }

        impl<C, $($($ty: Message + 'static,)?)*> TransformInto<Dyn<dyn $ident<$($($ty,)?)*>>> for C
        where
            C: InboxType $($( + Accept<$ty> )?)*,
            C::Channel: Sized
        {
            fn transform_into(channel: Arc<Self::Channel>) -> Arc<dyn Channel> {
                <C::Channel as Channel>::into_dyn(channel)
            }
        }
    )*};
}

create_dynamic_actor_types! {
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


