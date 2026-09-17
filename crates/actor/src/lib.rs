//! Declarative actor implementation on top of `zestors-runtime`.
//!
//! An actor is any type implementing [`Actor`], whose [`Actor::run`] owns the
//! actor's event loop end to end. [`ActorExt`] provides the ergonomics built
//! on top of it: [`ActorExt::spawn`]/[`ActorExt::spawn_with`] to start the
//! actor, and [`ActorExt::map_actor_exit`]/[`ActorExt::wrap_actor`] to adapt
//! its behavior.
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
//! An [`ActorBlueprint`] is a reusable recipe for producing an actor,
//! together with the default [`RestartMode`] a supervisor should apply to it.

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
    pub use crate::actor::Actor;
    pub use crate::blueprint::ActorBlueprint;
    pub use crate::handler::Handler;
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
