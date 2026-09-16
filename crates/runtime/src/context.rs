use crate::*;
use std::marker::PhantomData;
use type_sets::AsTypeSet;

/// The context of an actor reference.
///
/// This is either:
/// 1. statically typed using the actor's [`Interface`].
/// 2. dynamically typed using a [`Dyn`] [set](type_sets).
pub trait Context: 'static {
    /// The underlying [set](type_sets) of messages that this actor accepts.
    type Set: AsTypeSet + 'static;
}

impl<I: Interface> Context for I {
    type Set = I::Set;
}

/// A marker-type for providing an actor with a dynamic [`Context`].
///
/// # Usage
/// - `Dyn` / `Dyn<()>`: Doesn't accept any messages
/// - `Dyn<(MsgA,)>`: Accepts only `MsgA`
/// - `Dyn<(MsgA, .., MsgX)>`: Accepts messages A..X
pub struct Dyn<T = ()>(PhantomData<fn() -> T>);

impl<S: AsTypeSet + 'static> Context for Dyn<S> {
    type Set = S;
}
