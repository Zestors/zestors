pub use zestors_core::actor_kind::*;



/// Macro for writing dynamic [channel definitions](ActorKind):
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
        $crate::actor_kind::Dyn<dyn $crate::actor_kind::AcceptsNone>
    };
    ($ty1:ty) => {
        $crate::actor_kind::Dyn<dyn $crate::actor_kind::AcceptsOne<$ty1>>
    };
    ($ty1:ty, $ty2:ty) => {
        $crate::actor_kind::Dyn<dyn $crate::actor_kind::AcceptsTwo<$ty1, $ty2>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty) => {
        $crate::actor_kind::Dyn<dyn $crate::actor_kind::AcceptsThree<$ty1, $ty2, $ty3>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty) => {
        $crate::actor_kind::Dyn<dyn $crate::actor_kind::AcceptsFour<$ty1, $ty2, $ty3, $ty4>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty) => {
        $crate::actor_kind::Dyn<dyn $crate::actor_kind::AcceptsFive<$ty1, $ty2, $ty3, $ty4, $ty5>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty) => {
        $crate::actor_kind::Dyn<dyn $crate::actor_kind::AcceptsSix<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty) => {
        $crate::actor_kind::Dyn<dyn $crate::actor_kind::AcceptsSeven<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty) => {
        $crate::actor_kind::Dyn<dyn $crate::actor_kind::AcceptsEight<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty, $ty9:ty) => {
        $crate::actor_kind::Dyn<dyn $crate::actor_kind::AcceptsNine<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8, $ty9>>
    };
    ($ty1:ty, $ty2:ty, $ty3:ty, $ty4:ty, $ty5:ty, $ty6:ty, $ty7:ty, $ty8:ty, $ty9:ty, $ty10:ty) => {
        $crate::actor_kind::Dyn<dyn $crate::actor_kind::AcceptsTen<$ty1, $ty2, $ty3, $ty4, $ty5, $ty6, $ty7, $ty8, $ty9, $ty10>>
    };
}

pub use Accepts;

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