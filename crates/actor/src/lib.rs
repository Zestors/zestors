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
//! together with the default restart policy ([`RestartMode`]/
//! [`RestartIntensity`]) a supervisor should apply to it.

mod actor;
use std::{collections::VecDeque, time::Duration};

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
use tokio::time::Instant;
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

/// Limits how many times an actor may be restarted within a sliding time
/// window, to prevent an actor that keeps failing immediately from restarting
/// in a tight, endless loop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RestartIntensity {
    /// The maximum number of restarts allowed within `within`.
    pub max_restarts: u16,

    /// The sliding time window over which `max_restarts` is counted.
    pub within: Duration,
}

impl Default for RestartIntensity {
    fn default() -> Self {
        Self {
            max_restarts: 3,
            within: Duration::from_mins(5),
        }
    }
}

impl RestartIntensity {
    /// Records a restart attempt against `restarts` (a history of previous
    /// restart timestamps, oldest first), dropping entries that have aged out
    /// of the window. Returns `false` without recording anything if
    /// `max_restarts` has already been reached within `within`.
    pub fn allow_restart(&self, restarts: &mut VecDeque<Instant>) -> bool {
        if self.max_restarts == u16::MAX && self.within.is_zero() {
            return true;
        }

        let now = Instant::now();

        while let Some(front) = restarts.front() {
            if now.duration_since(*front) > self.within {
                restarts.pop_front();
            } else {
                break;
            }
        }

        if restarts.len() >= self.max_restarts as usize {
            return false;
        }

        restarts.push_back(now);
        true
    }

    /// Creates a [`RestartIntensity`] allowing `max_restarts` within the
    /// default 5-minute window.
    pub fn restarts(max_restarts: u16) -> Self {
        Self {
            max_restarts,
            within: Duration::from_mins(5),
        }
    }

    /// Sets [`RestartIntensity::within`].
    pub fn within(mut self, within: Duration) -> Self {
        self.within = within;
        self
    }

    /// Creates a new [`RestartIntensity`] allowing `max_restarts` within `within`.
    pub fn new(max_restarts: u16, within: Duration) -> Self {
        Self {
            max_restarts,
            within,
        }
    }

    /// Creates a [`RestartIntensity`] that never limits restarts.
    pub fn infinite() -> Self {
        Self {
            max_restarts: u16::MAX,
            within: Duration::ZERO,
        }
    }
}

pub use zestors_codegen::HandlerInterface;
