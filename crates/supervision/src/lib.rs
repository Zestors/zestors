//! Shared building blocks for `zestors` supervision trees, in the OTP sense.
//!
//! This crate holds the spec/config/snapshot types and query messages that
//! describe *what* a supervisor supervises and *how* it may be restarted; the
//! `Supervisor` actor that actually starts,
//! watches, and restarts children lives in the `zestors-supervisor` crate.
//!
//! - [`ChildSpec`] pairs a child's blueprint with the [`ChildConfig`]
//!   (restart mode/intensity, timeouts) a supervisor applies to it, and owns
//!   the [`Pid`](zestors_runtime::Pid)/channel under which the child is
//!   registered.
//! - [`ChildDescription`] is a serializable snapshot of a child's identity and
//!   configuration, as returned by [`GetChildren`].
//! - [`RestartIntensity`] is a sliding-window restart budget used to prevent
//!   an actor that keeps failing immediately from restarting in a tight,
//!   endless loop.
//! - [`Start`]/[`DynStarter`] turn an
//!   [`ActorBlueprint`](zestors_actor::ActorBlueprint) into something that
//!   can (re)spawn an actor on an already-registered
//!   [`StrongAddress`](zestors_runtime::StrongAddress), and
//!   [`BlueprintSupervisionExt`] adds ergonomic `pid`/`with_rand_pid` helpers
//!   to every blueprint.
//! - [`messages`] holds the request/response types used to query a running
//!   supervisor (its children and its health).
//! - [`SupervisionTree`] recursively walks a supervisor and its descendants
//!   into a serializable snapshot of the whole tree.

mod _prelude {
    pub use crate::*;
    pub use serde::{Deserialize, Serialize};
    pub use std::fmt::{Debug, Display};
    pub use std::time::Duration;
    pub use zestors_actor::*;
    pub use zestors_interface::prelude::*;
    pub use zestors_runtime::prelude::*;
}

mod childspec;
pub use childspec::*;

mod start;
pub use start::*;

mod strategy;
pub use strategy::*;

pub mod messages;
pub(crate) use messages::*;

mod tree;
pub use tree::*;

pub mod prelude {
    pub use crate::childspec::ChildSpec;
}
