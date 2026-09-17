//! Supervision trees for `zestors` actors, in the OTP sense: a
//! [`Supervisor`] starts and watches a set of children — each described by a
//! [`ChildSpec`] — and restarts them according to a [`SupervisionStrategy`]
//! and a [`RestartMode`](zestors_actor::RestartMode)/[`RestartIntensity`]
//! policy when they exit.
//!
//! - [`SupervisorBlueprint`] builds a [`Supervisor`]: its children, its
//!   [`SupervisionStrategy`], its restart intensity, and an optional
//!   [`SupervisorSource`] for a dynamically-managed child set.
//! - [`ChildSpec`] pairs a child's blueprint with the [`ChildConfig`]
//!   (restart mode/intensity, timeouts) a supervisor applies to it.
//! - [`Node`] runs a single root [`Supervisor`] as an entire program: it
//!   starts it, restarts it if it crashes, and shuts it down gracefully on a
//!   Ctrl+C/SIGTERM.
//! - [`messages`] holds the request/response types used to talk to a running
//!   [`Supervisor`] (fetching its children, checking its health, registering
//!   or deregistering a child at runtime), and [`SupervisionTree`]
//!   recursively walks a supervisor and its descendants into a snapshot of
//!   the whole tree.

mod _prelude {
    pub use crate::*;
    pub use rootcause::Report;
    pub use serde::{Deserialize, Serialize};
    pub use std::fmt::{Debug, Display};
    pub use std::time::Duration;
    pub use zestors_actor::*;
    pub use zestors_interface::prelude::*;
    pub use zestors_runtime::prelude::*;
}

mod childspec;
pub use childspec::*;

mod supervisor;
pub use supervisor::*;

mod start;
pub use start::*;

mod strategy;
pub use strategy::*;

pub mod messages;
pub(crate) use messages::*;

mod node;
pub use node::*;

mod tree;
pub use tree::*;

pub mod prelude {
    pub use crate::childspec::ChildSpec;
    pub use crate::strategy::SupervisionStrategy;
    pub use crate::supervisor::{SupervisorBlueprint, SupervisorInterface};
}
