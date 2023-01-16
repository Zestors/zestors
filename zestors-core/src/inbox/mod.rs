use std::sync::Arc;

use crate::*;

/// Anything that can be passed along as the argument to the spawn function.
pub trait InboxKind: ActorKind + ActorRef<ActorKind = Self> + Send + 'static {
    type Cfg;

    /// Sets up the channel, preparing for x processes to be spawned.
    fn setup_channel(
        config: Self::Cfg,
        process_count: usize,
        address_count: usize,
        actor_id: ActorId,
    ) -> (Arc<Self::Channel>, Link);

    /// Creates another inbox from the channel, without adding anything to its process count.
    fn new(channel: Arc<Self::Channel>) -> Self;
}