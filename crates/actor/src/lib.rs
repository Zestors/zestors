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
    pub use crate::state::HandlerState;
}

use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use zestors_runtime::ExitStatus;

/// Controls when an actor should be restarted
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RestartMode {
    Always,
    OnError,
    Never,
}

impl Default for RestartMode {
    fn default() -> Self {
        RestartMode::OnError
    }
}

impl RestartMode {
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RestartIntensity {
    pub max_restarts: u16,
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

    pub fn restarts(max_restarts: u16) -> Self {
        Self {
            max_restarts,
            within: Duration::from_mins(5),
        }
    }

    pub fn within(mut self, within: Duration) -> Self {
        self.within = within;
        self
    }

    pub fn new(max_restarts: u16, within: Duration) -> Self {
        Self {
            max_restarts,
            within,
        }
    }

    pub fn infinite() -> Self {
        Self {
            max_restarts: u16::MAX,
            within: Duration::ZERO,
        }
    }
}

pub use zestors_codegen::HandlerInterface;
