use crate::all::*;

/// [`Accept`] is implemented for any [`ActorType`] that accepts the [`Message`] `M`.
pub trait Accept<M: Message>: ActorType {
    type SendFut<'a>: Future<Output = Result<M::Returned, SendError<M>>> + Send + 'a;
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
        $crate::channel::Dyn<dyn $crate::channel::dyn_actor_types::AcceptsNone>
    };
    ($ty1:ty) => {
        $crate::channel::Dyn<dyn $crate::channel::dyn_actor_types::AcceptsOne<$ty1>>
    };
    ($ty1:ty, $ty2:ty) => {
        $crate::channel::Dyn<dyn $crate::channel::dyn_actor_types::AcceptsTwo<$ty1, $ty2>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty) => {
        $crate::channel::Dyn<dyn $crate::channel::dyn_actor_types::AcceptsThree<$ty1, $ty2, $ty3>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty) => {
        $crate::channel::Dyn<dyn $crate::channel::dyn_actor_types::AcceptsFour<$ty1, $ty2, $ty3, $ty4>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty) => {
        $crate::channel::Dyn<dyn $crate::channel::dyn_actor_types::AcceptsFive<$ty1, $ty2, $ty3, $ty4, $ty5>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty) => {
        $crate::channel::Dyn<dyn $crate::channel::dyn_actor_types::AcceptsSix<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty) => {
        $crate::channel::Dyn<dyn $crate::channel::dyn_actor_types::AcceptsSeven<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty) => {
        $crate::channel::Dyn<dyn $crate::channel::dyn_actor_types::AcceptsEight<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty, $ty9:ty) => {
        $crate::channel::Dyn<dyn $crate::channel::dyn_actor_types::AcceptsNine<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8, $ty9>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty, $ty9:ty, $ty10:ty) => {
        $crate::channel::Dyn<dyn $crate::channel::dyn_actor_types::AcceptsTen<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8, $ty9, $ty10>>
    };
}
pub use Accepts;
use futures::Future;
