//! An OTP-style [`Supervisor`] actor for `zestors`: it starts and watches a
//! set of children — each described by a
//! [`ChildSpec`](zestors_supervision::ChildSpec) — and restarts them according
//! to a [`SupervisionStrategy`] and a
//! [`RestartIntensity`](zestors_supervision::RestartIntensity) policy when they
//! exit.
//!
//! - [`SupervisorBlueprint`] builds a [`Supervisor`]: its children, its
//!   [`SupervisionStrategy`], its restart intensity, and an optional
//!   [`SupervisorSource`] for a dynamically-managed child set.
//! - [`SupervisionStrategy`] selects the restart policy:
//!   [`OneForOne`](SupervisionStrategy::OneForOne),
//!   [`OneForAll`](SupervisionStrategy::OneForAll), or
//!   [`RestForOne`](SupervisionStrategy::RestForOne).
//! - [`SupervisorSource`]/[`InMemorySupervisorSource`] let a running
//!   supervisor add and remove children at runtime.
//! - [`SupervisorInterface`] is the supervisor's message interface, answering
//!   the child/health queries from `zestors-supervision` as well as the
//!   [`RegisterChild`](messages::RegisterChild)/[`DeregisterChild`](messages::DeregisterChild)
//!   messages defined in [`messages`].
//! - [`Node`] runs a single root [`Supervisor`] as an entire program: it
//!   starts it, restarts it if it crashes, and shuts it down gracefully on a
//!   Ctrl+C/SIGTERM.
//!
//! The shared building blocks — [`ChildSpec`](zestors_supervision::ChildSpec),
//! [`ChildConfig`](zestors_supervision::ChildConfig),
//! [`ChildDescription`](zestors_supervision::ChildDescription),
//! [`RestartIntensity`](zestors_supervision::RestartIntensity), the
//! child/health query messages, and
//! [`SupervisionTree`](zestors_supervision::SupervisionTree) — all live in the
//! `zestors-supervision` crate.
//!
//! # Example
//!
//! Building on `zestors-supervision`'s [`ChildSpec`](zestors_supervision::ChildSpec)
//! example: a [`SupervisorBlueprint`] collects one or more specs, and
//! instantiating it produces a [`Supervisor`] - an ordinary [`Actor`] like
//! any other, so it's spawned and messaged the same way.
//!
//! ```
//! use zestors::interface::{Envelope, Interface, Message};
//! use zestors::prelude::*;
//! use zestors::supervision::ChildSpec;
//! use zestors::supervision::messages::GetChildren;
//! use zestors::supervisor::Supervisor;
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
//! let blueprint = Supervisor::blueprint().child(ChildSpec::create_rand_pid(Worker));
//! let supervisor = blueprint.instantiate().await.unwrap().spawn_rand();
//!
//! // `SupervisorInterface` answers `GetChildren`/`GetHealth` (from
//! // `zestors-supervision`) without needing to stop the tree to inspect it.
//! let children = supervisor.call(GetChildren).await.unwrap();
//! assert_eq!(children.len(), 1);
//!
//! supervisor.signal_shutdown();
//! # }
//! ```
//!
//! This spawns the supervisor directly; [`Node`] is for the common case of
//! running one *root* supervisor as an entire program, adding Ctrl+C/SIGTERM
//! handling and a bounded shutdown on top.

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
    pub use std::fmt::Debug;
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
