//! Declarative actor implementation on top of `zestors-runtime`.
//!
//! An actor is any type implementing [`Actor`], whose [`Actor::run`] owns the
//! actor's event loop end to end. [`ActorExt`] provides the ergonomics built
//! on top of it: [`ActorExt::spawn`]/[`ActorExt::spawn_rand`] to start the
//! actor (mirroring [`zestors_runtime::spawn`]/[`zestors_runtime::spawn_rand`]),
//! and [`ActorExt::map_actor_exit`]/[`ActorExt::wrap_actor`] to adapt its
//! behavior.
//!
//! Most actors are easier to write via [`Handler`], which implements
//! [`Actor`] automatically from a set of lifecycle hooks ([`Handler::init`],
//! [`Handler::exit`], [`Handler::on_shutdown`],
//! [`on_suspend`](Handler::on_suspend), [`on_resume`](Handler::on_resume))
//! and per-message [`Handle<M>`] implementations. Each call is given a
//! [`HandlerContext`], the handler's view of its own actor.
//! [`Handler::next_event`] (optionally backed by [`BasicScheduler`]) lets a
//! handler additionally react to arbitrary futures alongside its messages and
//! signals.
//!
//! An [`Blueprint`] is a reusable recipe for producing an actor,
//! together with the default [`RestartMode`] a supervisor should apply to it.
//!
//! # Example
//!
//! A counter actor, built with [`Handler`] instead of implementing [`Actor`]
//! directly: `#[derive(Interface, HandlerInterface)]` on the message set
//! wires each variant to a [`Handle<M>`] implementation on `Counter`, and
//! [`Handler::Interface`] ties the two together.
//!
//! ```
//! use zestors::interface::{Envelope, Interface, Message, Request};
//! use zestors::prelude::*;
//!
//! #[derive(Message, Debug)]
//! struct Increment;
//!
//! #[derive(Message, Debug)]
//! #[msg(reply = u32)]
//! struct GetCount;
//!
//! #[derive(Interface, HandlerInterface, Debug)]
//! enum CounterInterface {
//!     Increment(Envelope<Increment>),
//!     GetCount(Envelope<GetCount>),
//! }
//!
//! #[derive(Debug, Clone)]
//! struct Counter {
//!     count: u32,
//! }
//!
//! impl Handler for Counter {
//!     type Interface = CounterInterface;
//! }
//!
//! impl Handle<Increment> for Counter {
//!     async fn handle(
//!         &mut self,
//!         _ctx: HandlerContext<'_, Self>,
//!         _msg: Increment,
//!         _req: (),
//!     ) -> Result<(), rootcause::Report> {
//!         self.count += 1;
//!         Ok(())
//!     }
//! }
//!
//! impl Handle<GetCount> for Counter {
//!     async fn handle(
//!         &mut self,
//!         _ctx: HandlerContext<'_, Self>,
//!         _msg: GetCount,
//!         req: Request<u32>,
//!     ) -> Result<(), rootcause::Report> {
//!         let _ = req.reply(self.count);
//!         Ok(())
//!     }
//! }
//!
//! # #[tokio::main]
//! # async fn main() {
//! // `ActorExt::spawn_rand` starts the actor on a fresh `Name`, same as
//! // `zestors_runtime::spawn_rand` would for a hand-written `Actor`.
//! let child = Counter { count: 0 }.spawn_rand();
//!
//! for _ in 0..5 {
//!     child.cast(Increment).await.unwrap();
//! }
//! assert_eq!(child.call(GetCount).await.unwrap(), 5);
//!
//! child.signal_shutdown();
//! # }
//! ```
//!
//! Deriving `Handler` this way is optional - implementing [`Actor`] directly
//! gives full control over the event loop (reading messages, signals, and
//! other futures in whatever order and combination the actor needs), at the
//! cost of writing that loop by hand instead of getting it for free.

mod actor;

pub use actor::*;

mod blueprint;
pub use blueprint::*;

mod handler;
pub use handler::*;

mod state;
pub use state::*;

mod scheduler;
pub use scheduler::*;

pub mod prelude {
    pub use crate::actor::{Actor, ActorExt};
    pub use crate::blueprint::{Blueprint, BlueprintExt};
    pub use crate::handler::{Handle, Handler};
    pub use crate::scheduler::HandledBy;
    pub use crate::state::HandlerContext;
}

use serde::{Deserialize, Serialize};
use zestors_runtime::ExitStatus;

/// Controls when an actor should be restarted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RestartMode {
    /// Always restart, regardless of how the actor exited.
    Always,

    /// Restart only if the actor exited abnormally (see [`RestartMode::should_restart`]).
    #[default]
    OnError,

    /// Never restart.
    Never,
}

impl RestartMode {
    /// Returns `true` if an actor that exited with `exit` should be restarted
    /// under this mode.
    pub fn should_restart(&self, exit: &ExitStatus) -> bool {
        match (self, exit) {
            (RestartMode::Always, _) => true,
            (RestartMode::Never, _) => false,
            (
                RestartMode::OnError,
                ExitStatus::Panicked | ExitStatus::Aborted | ExitStatus::UnhandledError,
            ) => true,
            (RestartMode::OnError, ExitStatus::Normal) => false,
        }
    }
}

pub use zestors_codegen::HandlerInterface;
