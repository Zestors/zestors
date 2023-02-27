/*!
Module containing all dynamic [actor types](ActorType).
*/
use crate::all::*;
use std::{any::TypeId, sync::Arc};

macro_rules! create_dynamic_actor_types {
    ($($actor_ty:ident $(<$( $msg:ident ),*>)?),*) => {$(
        // creates the dynamic actor-type trait which implements FromPayload<..> as a marker-trait.
        /// Used as an argument to a [`DynActor<dyn _>`].
        pub trait $actor_ty< $($($msg: Message,)?)*>: std::fmt::Debug + $($( FromPayload<$msg> + )?)* {}

        // Implement the dynamic actor-type for DynActor<dyn _>
        impl<$($($msg: Message + 'static,)?)*> DynActorType for DynActor<dyn $actor_ty< $($($msg,)?)*>> {
            fn msg_ids() -> Box<[TypeId]> {
                Box::new([$($(TypeId::of::<$msg>(),)?)*])
            }
        }

        // Any sized channel can transform into this DynActor<dyn _>, as long as it impl FromPayload<M>
        // for all the messages
        impl<D, $($($msg: Message + 'static,)?)*> TransformInto<DynActor<dyn $actor_ty<$($($msg,)?)*>>> for DynActor<D>
        where
            D: ?Sized $($( + FromPayload<$msg> )?)*
        {
            fn transform_into(channel: Arc<Self::Channel>) -> Arc<dyn Channel> {
                channel
            }
        }

        // Any sized channel can transform into this DynActor<dyn _>, as long as it implements
        // Accept<M> all the messages
        impl<I, $($($msg: Message + 'static,)?)*> TransformInto<DynActor<dyn $actor_ty<$($($msg,)?)*>>> for I
        where
            I: ActorInbox $($( + Accept<$msg> )?)*,
            I::Channel: Sized
        {
            fn transform_into(channel: Arc<Self::Channel>) -> Arc<dyn Channel> {
                <I::Channel as Channel>::into_dyn(channel)
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
