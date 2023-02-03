use crate::*;
use futures::{future::BoxFuture, Future};
use std::{any::TypeId, marker::PhantomData, sync::Arc};

//------------------------------------------------------------------------------------------------
//  ActorType
//------------------------------------------------------------------------------------------------

/// A channel-definition is part of any [Address] or [Child], and defines the [Channel] used in the
/// actor and which messages the actor [Accept]. The different channel-definitions are:
/// - A [`Protocol`]. (sized)
/// - A [`Halter`]. (sized)
/// - A [`Dyn<_>`] type using the [`Accepts!`] macro. (dynamic)
pub trait ActorType {
    type Channel: Channel + ?Sized;
}

impl<D: ?Sized> ActorType for Dyn<D> {
    type Channel = dyn Channel;
}

//------------------------------------------------------------------------------------------------
//  DynActorType
//------------------------------------------------------------------------------------------------

/// A specialization of [`ActorType`] for dynamic channels only.
pub trait DynActorType: ActorType<Channel = dyn Channel> {
    fn msg_ids() -> Box<[TypeId]>;
}

//------------------------------------------------------------------------------------------------
//  Accept
//------------------------------------------------------------------------------------------------

/// Accept is implemented for any [`ActorType`] which accepts the message. If a type implements,
/// then messages of type `M` can be sent to it.
///
/// This trait can be implemented for:
/// - [Dynamic actor kinds](DynActorType) (See [`Dyn`]).
/// - Sized actor kinds which implement [`InboxType`].
pub trait Accept<M: Message>: ActorType {
    type SendFut<'a>: Future<Output = Result<M::Returned, SendError<M>>> + Send + 'a;
    fn try_send(channel: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>>;
    fn send_now(channel: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>>;
    fn send_blocking(channel: &Self::Channel, msg: M) -> Result<M::Returned, SendError<M>>;
    fn send(channel: &Self::Channel, msg: M) -> Self::SendFut<'_>;
}

impl<M, D> Accept<M> for Dyn<D>
where
    Self: DynActorType + TransformInto<Dyn<dyn AcceptsNone>>,
    M: Message + Send + 'static,
    M::Payload: Send,
    M::Returned: Send,
    D: ?Sized,
{
    fn try_send(channel: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>> {
        channel.try_send_checked(msg).map_err(|e| match e {
            TrySendCheckedError::Full(msg) => TrySendError::Full(msg),
            TrySendCheckedError::Closed(msg) => TrySendError::Closed(msg),
            TrySendCheckedError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    fn send_now(channel: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>> {
        channel.send_now_unchecked(msg).map_err(|e| match e {
            TrySendCheckedError::Full(msg) => TrySendError::Full(msg),
            TrySendCheckedError::Closed(msg) => TrySendError::Closed(msg),
            TrySendCheckedError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    fn send_blocking(channel: &Self::Channel, msg: M) -> Result<M::Returned, SendError<M>> {
        channel.send_blocking_checked(msg).map_err(|e| match e {
            SendCheckedError::Closed(msg) => SendError(msg),
            SendCheckedError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    type SendFut<'a> = BoxFuture<'a, Result<M::Returned, SendError<M>>>;

    fn send(channel: &Self::Channel, msg: M) -> Self::SendFut<'_> {
        Box::pin(async move {
            channel.send_checked(msg).await.map_err(|e| match e {
                SendCheckedError::Closed(msg) => SendError(msg),
                SendCheckedError::NotAccepted(_) => {
                    panic!("Sent message which was not accepted by actor")
                }
            })
        })
    }
}

//------------------------------------------------------------------------------------------------
//  Dyn
//------------------------------------------------------------------------------------------------

/// An [DynActorType] with a dynamic [Channel] that accepts specific messages:
/// - `Dyn<dyn AcceptsNone>` -> Accepts no messages
/// - `Dyn<dyn AcceptsTwo<u32, u64>` -> Accepts two messages:` u32` and `u64`.
///
/// Any [InboxType] which accepts these messages can be transformed into a dynamic channel using
/// [TransformInto].
#[derive(Debug)]
pub struct Dyn<T: ?Sized>(PhantomData<*const T>);

unsafe impl<T: ?Sized> Send for Dyn<T> {}
unsafe impl<T: ?Sized> Sync for Dyn<T> {}

//------------------------------------------------------------------------------------------------
//  TransformInto
//------------------------------------------------------------------------------------------------

/// Indicates that an [ActorType] can transform into another one.
pub trait TransformInto<T: ActorType>: ActorType {
    fn transform_into(channel: Arc<Self::Channel>) -> Arc<T::Channel>;
}

//------------------------------------------------------------------------------------------------
//  dynamic protocol types
//------------------------------------------------------------------------------------------------

macro_rules! define_dynamic_protocol_types {
    ($($ident:ident $(<$( $ty:ident ),*>)?),*) => {$(
        /// A type `T` within [`Dyn<T>`].
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
