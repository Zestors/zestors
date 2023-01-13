use std::{fmt::{Display, Debug}, sync::atomic::{AtomicU64, Ordering}};

/// An actor-id is a unique id incrementally given to every actor when it is spawned.
#[derive(PartialEq, Eq, Debug, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct ActorId(u64);

impl Display for ActorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as Debug>::fmt(&self, f)
    }
}

impl ActorId {
    pub(super) fn generate_new() -> Self {
        static NEXT_ACTOR_ID: AtomicU64 = AtomicU64::new(0);
        ActorId(NEXT_ACTOR_ID.fetch_add(1, Ordering::AcqRel))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn actor_ids_increase() {
        let mut old_id = ActorId::generate_new();
        for _ in 0..100 {
            let id = ActorId::generate_new();
            assert!(id > old_id);
            old_id = id;
        }
    }
}