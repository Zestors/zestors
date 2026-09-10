use super::*;
use std::marker::PhantomData;
use type_sets::AsTypeSet;

pub trait Context: 'static {
    type Set: AsTypeSet + 'static;
}

impl<I: Interface> Context for I {
    type Set = I::Set;
}

/// A marker-type for providing an actor with a dynamic [`Context`].
///
/// This is in contrast to a static context, provided by [`Interface`].
pub struct Dyn<T>(PhantomData<fn() -> T>);

impl<S: AsTypeSet + 'static> Context for Dyn<S> {
    type Set = S;
}
