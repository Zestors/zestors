use type_sets::AsTypeSet;

use super::*;

pub trait Context: 'static {
    type Set: AsTypeSet + 'static;
}

impl<I: Interface> Context for I {
    type Set = I::Set;
}

impl<S: AsTypeSet + 'static> Context for Set<S> {
    type Set = S;
}
