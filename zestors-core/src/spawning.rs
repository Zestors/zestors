use crate::*;
use std::sync::Arc;

/// Implemented for any reference to an actor, which allows interaction with the actor.
/// For example it allows you to close the inbox, query it's capacity, halt it or send messages.
pub trait ActorRef {
    type ActorType: ActorType;
    fn channel(&self) -> &Arc<<Self::ActorType as ActorType>::Channel>;
}


