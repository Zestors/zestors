use crate::*;

/// An [ActorType] signifies the type that an actor can be. This can be one of:
/// - A [Protocol]
/// - A [Dyn<_>] type.
pub trait ActorType {
    /// The underlying channel used.
    type Channel: DynChannel + ?Sized;
}

impl<P: Protocol> ActorType for P {
    type Channel = tiny_actor::Channel<P>;
}

impl<D: ?Sized> ActorType for Dyn<D> {
    type Channel = dyn DynChannel;
}