use std::{
    fmt::{Debug, Display},
    sync::atomic::{AtomicU64, Ordering},
};

/// An actor-id is an incrementally-generated id, unique per actor.
#[derive(PartialEq, Eq, Debug, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct ActorId(u64);

impl ActorId {
    /// Generate a new unique actor-id. 
    /// 
    /// This is the only way to create actor-id's. (Except for Clone/Copy)
    pub fn generate() -> Self {
        static NEXT_ACTOR_ID: AtomicU64 = AtomicU64::new(0);
        ActorId(NEXT_ACTOR_ID.fetch_add(1, Ordering::AcqRel))
    }

    /// Convert the actor-id to a u64.
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl Display for ActorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as Debug>::fmt(&self, f)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn actor_ids_increase() {
        let mut old_id = ActorId::generate();
        for _ in 0..100 {
            let id = ActorId::generate();
            assert!(id > old_id);
            old_id = id;
        }
    }
}
