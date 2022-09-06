use crate::*;
use tiny_actor::Channel;

mod accepts;
mod dynamic;
pub use {accepts::*, dynamic::*};

/// An `ActorType` signifies the type that an actor can be. This can be either
/// a [Protocol] or a [Dyn<_>] type.
pub trait ActorType {
    type Channel: BoxChannel + ?Sized;
}

impl<P> ActorType for P
where
    P: Protocol,
{
    type Channel = Channel<P>;
}

impl<D: ?Sized> ActorType for Dyn<D> {
    type Channel = dyn BoxChannel;
}
