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
//!   [`Blueprint`](zestors_actor::Blueprint) into something that
//!   can (re)spawn an actor on an already-registered
//!   [`StrongAddress`](zestors_runtime::StrongAddress), and
//!   [`BlueprintSupervisionExt`] adds ergonomic `pid`/`with_rand_pid` helpers
//!   to every blueprint.
//! - [`messages`] holds the request/response types used to query a running
//!   supervisor (its children and its health).
//! - [`SupervisionTree`] recursively walks a supervisor and its descendants
//!   into a serializable snapshot of the whole tree.
//!
//! # Example
//!
//! Any actor that is `Clone + Debug` (a [`Handler`](zestors_actor::Handler)
//! usually is) implements [`Blueprint`](zestors_actor::Blueprint)
//! automatically, so it can go straight into a [`ChildSpec`]:
//!
//! ```
//! use std::time::Duration;
//! use zestors::actor::RestartMode;
//! use zestors::interface::{Envelope, Interface, Message};
//! use zestors::prelude::*;
//! use zestors::supervision::ChildSpec;
//!
//! #[derive(Message, Debug)]
//! struct Ping;
//!
//! #[derive(Interface, HandlerInterface, Debug)]
//! enum WorkerInterface {
//!     Ping(Envelope<Ping>),
//! }
//!
//! #[derive(Debug, Clone)]
//! struct Worker;
//!
//! impl Handler for Worker {
//!     type Interface = WorkerInterface;
//! }
//!
//! impl Handle<Ping> for Worker {
//!     async fn handle(
//!         &mut self,
//!         _ctx: HandlerContext<'_, Self>,
//!         _msg: Ping,
//!         _req: (),
//!     ) -> Result<(), rootcause::Report> {
//!         Ok(())
//!     }
//! }
//!
//! # #[tokio::main]
//! # async fn main() {
//! let spec = ChildSpec::create_rand_pid(Worker)
//!     .with_mode(RestartMode::Always)
//!     .with_abort_timeout(Duration::from_secs(1));
//!
//! // `start` instantiates the blueprint and spawns it - this is what a
//! // `Supervisor` (in `zestors-supervisor`) calls, and retries according to
//! // `cfg().restart_mode`, whenever the child exits.
//! let child = spec.start().await.unwrap();
//! child.cast(Ping).await.unwrap();
//! child.signal_shutdown();
//! # }
//! ```
//!
//! A [`ChildSpec`] on its own is just a recipe plus a reserved [`Pid`]; it
//! doesn't watch the child or restart it. That behavior belongs to the
//! `Supervisor` actor in `zestors-supervisor`, which holds a set of specs and
//! calls `start`/`restart` on them according to a `SupervisionStrategy`.

mod _prelude {
    pub use crate::*;
    pub use serde::{Deserialize, Serialize};
    pub use std::fmt::{Debug, Display};
    pub use std::time::Duration;
    pub use zestors_actor::*;
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

#[doc(hidden)]
pub mod prelude {
    pub use crate::{BlueprintSupervisionExt as _, childspec::ChildSpec};
}
