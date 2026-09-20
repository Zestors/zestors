//! An OTP-style [`Supervisor`] actor for `zestors`: it starts and monitors a
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
//!   starts it, and shuts it down gracefully on a Ctrl+C/SIGTERM. It exits
//!   whenever the supervisor does.
//!
//! The shared building blocks — [`ChildSpec`](zestors_supervision::ChildSpec),
//! [`ChildConfig`](zestors_supervision::ChildConfig),
//! [`ChildDescription`](zestors_supervision::ChildDescription),
//! [`RestartIntensity`](zestors_supervision::RestartIntensity), the
//! child/health query messages, and
//! [`SupervisionTree`](zestors_supervision::SupervisionTree) — all live in the
//! `zestors-supervision` crate.
//!
//! # Examples
//!
//! ## A standalone `Supervisor`
//!
//! A [`SupervisorBlueprint`] collects one or more
//! [`ChildSpec`](zestors_supervision::ChildSpec)s, each pairing a blueprint
//! with the [`Name`] it's registered under and the [`RestartMode`] the
//! supervisor should apply to it. Instantiating the blueprint produces a
//! [`Supervisor`] - an ordinary [`Actor`] like any other, so
//! [`BlueprintExt::start`]/[`start_rand`](BlueprintExt::start_rand) start it
//! the same way they would any other actor.
//!
//! ```
//! use zestors::actor::RestartMode;
//! use zestors::interface::{Envelope, Interface, Message};
//! use zestors::prelude::*;
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
//! let blueprint = Supervisor::blueprint().children([
//!     Worker.name("worker-a").unwrap().with_mode(RestartMode::Always),
//!     Worker.name("worker-b").unwrap().with_mode(RestartMode::Never),
//! ]);
//! let supervisor = blueprint.start_rand().await.unwrap();
//!
//! // A `Supervisor` only reports itself as `Running` once every child it
//! // started with has finished its own initialization.
//! supervisor.monitor_init().await.unwrap();
//!
//! // `SupervisorInterface` answers `GetChildren`/`GetHealth` (from
//! // `zestors-supervision`) without needing to stop the tree to inspect it.
//! let children = supervisor.call(GetChildren).await.unwrap();
//! assert_eq!(children.len(), 2);
//!
//! supervisor.signal_shutdown();
//! # }
//! ```
//!
//! This spawns the supervisor directly. Reach for [`Node`] instead when the
//! supervisor being started actually is the top of the tree for the whole
//! program.
//!
//! ## Running one as a program with `Node`
//!
//! [`Node`] takes a [`ChildSpec<SupervisorBlueprint>`](zestors_supervision::ChildSpec)
//! for a *root* supervisor and runs it as an entire program: [`Node::run`]
//! starts it, and shuts it down gracefully - on a Ctrl+C/SIGTERM in a
//! real program, or, as below, on an ordinary
//! [`ActorOps::signal_shutdown`] sent to its address like any other actor.
//!
//! ```
//! use zestors::interface::{Envelope, Interface, Message};
//! use zestors::prelude::*;
//! use zestors::supervision::messages::GetChildren;
//! use zestors::supervisor::{Node, Supervisor};
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
//! let node = Node::new(
//!     Supervisor::blueprint()
//!         .child(Worker.name("worker").unwrap())
//!         .rand_name(),
//! );
//!
//! // `Node::run` consumes the `Node`, so keep a handle to the root
//! // supervisor before handing it off.
//! let root = node.root_supervisor().address().clone();
//! let node_task = tokio::spawn(node.run());
//!
//! root.monitor_running().await;
//! let children = root.call(GetChildren).await.unwrap();
//! assert_eq!(children.len(), 1);
//!
//! // The root supervisor exiting on its own - here, because we asked it
//! // to - is a normal, successful stop for the whole node. (Once the
//! // supervisor is running, as it is here, signalling its address works.
//! // Before that, a signal is dropped, so wait for it as above.)
//! root.signal_shutdown();
//! assert!(node_task.await.unwrap().is_ok());
//! # }
//! ```
//!
//! In a real program, `node.run()` is usually just `.await`ed directly from
//! `main` instead of spawned onto a background task: the only reason to
//! reach for the root supervisor's address at all here is to trigger and
//! observe a controlled shutdown from the test itself, in place of the
//! Ctrl+C/SIGTERM a real deployment would send.

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

#[doc(hidden)]
pub mod prelude {
    pub use crate::SupervisionStrategy;
    pub use crate::{SupervisorBlueprint, SupervisorInterface};
}

pub mod messages {
    use crate::_prelude::*;
    use zestors_runtime::errors::DuplicateNameError;
    use zestors_supervision::ChildDescription;

    /// Registers a new child under a running supervisor. Fails if the spec's
    /// [`Name`] is already registered.
    #[derive(Message, Debug)]
    #[zestors(interface_path = "zestors_interface")]
    #[msg(reply = "Result<(), DuplicateNameError>")]
    pub struct RegisterChild(pub ChildSpec);

    /// Removes a child from a running supervisor (stopping it if it's alive),
    /// returning its [`ChildDescription`] if it was present.
    #[derive(Message, Debug)]
    #[zestors(interface_path = "zestors_interface")]
    #[msg(reply = "Option<ChildDescription>")]
    pub struct DeregisterChild(pub Name);
}
