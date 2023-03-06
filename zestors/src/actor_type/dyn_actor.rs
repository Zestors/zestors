#[allow(unused)]
use crate::all::*;

/// Macro for writing a [`struct@DynActor<dyn _>`]:
///
/// - `DynActor!()` = `DynActor<dyn AcceptsNone>`
/// - `DynActor!(u32, u64)` = `DynActor<dyn AcceptsTwo<u32, u64>`
#[macro_export]
macro_rules! DynActor {
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
pub use DynActor;
