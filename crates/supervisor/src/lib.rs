mod actor;
mod source;

pub use actor::*;
pub use source::*;

mod strategy;
pub use strategy::*;

mod node;
pub use node::*;

mod _prelude {
    pub use crate::*;
    pub use rootcause::Report;
    pub use serde::{Deserialize, Serialize};
    pub use std::fmt::{Debug, Display};
    pub use std::time::Duration;
    pub use zestors_actor::*;
    pub use zestors_interface::prelude::*;
    pub use zestors_runtime::prelude::*;
    pub use zestors_supervision::prelude::*;
}

pub mod prelude {
    pub use crate::SupervisionStrategy;
    pub use crate::{SupervisorBlueprint, SupervisorInterface};
}

pub mod messages {
    use crate::_prelude::*;
    use zestors_runtime::errors::DuplicatePidError;
    use zestors_supervision::ChildDescription;

    /// Registers a new child under a running supervisor. Fails if the spec's
    /// [`Pid`] is already registered.
    #[derive(Message, Debug)]
    #[msg(path = "zestors_interface", reply = "Result<(), DuplicatePidError>")]
    pub struct RegisterChild(pub ChildSpec);

    /// Removes a child from a running supervisor (stopping it if it's alive),
    /// returning its [`ChildDescription`] if it was present.
    #[derive(Message, Debug)]
    #[msg(path = "zestors_interface", reply = "Option<ChildDescription>")]
    pub struct DeregisterChild(pub Pid);
}
