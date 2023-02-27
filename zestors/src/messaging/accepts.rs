use crate::all::*;
use futures::{future::BoxFuture, Future};

/// [`Accept`] is implemented for any [`ActorType`] that accepts the [`Message`] `M`.
pub trait Accept<M: Message>: ActorType {
    type SendFut<'a>: Future<Output = Result<M::Returned, SendError<M>>> + Send;
    fn try_send(channel: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>>;
    fn force_send(channel: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>>;
    fn send_blocking(channel: &Self::Channel, msg: M) -> Result<M::Returned, SendError<M>>;
    fn send(channel: &Self::Channel, msg: M) -> Self::SendFut<'_>;
}

/// Macro for writing a dynamic [`ActorType`]:
///
/// - `Accepts![]` = `Dyn<dyn AcceptsNone>`
/// - `Accepts![u32, u64]` = `Dyn<dyn AcceptsTwo<u32, u64>`
///
/// These macros can be used as a generic argument to [children](Child) and [addresses](Address):
/// - `Address<Accepts![u32, String]>`
/// - `Child<_, Accepts![u32, String], _>`
#[macro_export]
macro_rules! Accepts {
    () => {
        $crate::actor_type::DynActor<dyn $crate::actor_type::dyn_types::AcceptsNone>
    };
    ($ty1:ty) => {
        $crate::actor_type::DynActor<dyn $crate::actor_type::dyn_types::AcceptsOne<$ty1>>
    };
    ($ty1:ty, $ty2:ty) => {
        $crate::actor_type::DynActor<dyn $crate::actor_type::dyn_types::AcceptsTwo<$ty1, $ty2>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty) => {
        $crate::actor_type::DynActor<dyn $crate::actor_type::dyn_types::AcceptsThree<$ty1, $ty2, $ty3>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty) => {
        $crate::actor_type::DynActor<dyn $crate::actor_type::dyn_types::AcceptsFour<$ty1, $ty2, $ty3, $ty4>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty) => {
        $crate::actor_type::DynActor<dyn $crate::actor_type::dyn_types::AcceptsFive<$ty1, $ty2, $ty3, $ty4, $ty5>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty) => {
        $crate::actor_type::DynActor<dyn $crate::actor_type::dyn_types::AcceptsSix<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty) => {
        $crate::actor_type::DynActor<dyn $crate::actor_type::dyn_types::AcceptsSeven<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty) => {
        $crate::actor_type::DynActor<dyn $crate::actor_type::dyn_types::AcceptsEight<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty, $ty9:ty) => {
        $crate::actor_type::DynActor<dyn $crate::actor_type::dyn_types::AcceptsNine<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8, $ty9>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty, $ty9:ty, $ty10:ty) => {
        $crate::actor_type::DynActor<dyn $crate::actor_type::dyn_types::AcceptsTen<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8, $ty9, $ty10>>
    };
}
pub use Accepts;

pub trait AcceptExt<M: Message>: Accept<M> {
    fn try_request<F, E, R>(
        channel: &Self::Channel,
        msg: M,
    ) -> BoxFuture<'_, Result<R, TryRequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
    {
        Box::pin(async move {
            match Self::try_send(channel, msg) {
                Ok(rx) => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(e) => Err(TryRequestError::NoReply(e)),
                },
                Err(TrySendError::Closed(msg)) => Err(TryRequestError::Closed(msg)),
                Err(TrySendError::Full(msg)) => Err(TryRequestError::Full(msg)),
            }
        })
    }

    fn force_request<F, E, R>(
        channel: &Self::Channel,
        msg: M,
    ) -> BoxFuture<'_, Result<R, TryRequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
    {
        Box::pin(async move {
            match Self::force_send(channel, msg) {
                Ok(rx) => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(e) => Err(TryRequestError::NoReply(e)),
                },
                Err(TrySendError::Closed(msg)) => Err(TryRequestError::Closed(msg)),
                Err(TrySendError::Full(msg)) => Err(TryRequestError::Full(msg)),
            }
        })
    }

    fn request<F, E, R>(
        channel: &Self::Channel,
        msg: M,
    ) -> BoxFuture<'_, Result<R, RequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
    {
        Box::pin(async move {
            match Self::send(channel, msg).await {
                Ok(rx) => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(e) => Err(RequestError::NoReply(e)),
                },
                Err(SendError(msg)) => Err(RequestError::Closed(msg)),
            }
        })
    }
}
impl<M: Message, T> AcceptExt<M> for T where T: Accept<M> {}
