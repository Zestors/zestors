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
    pub use crate::blueprint::Blueprint;
    pub use crate::handler::Handler;
    pub use crate::scheduler::HandledBy;
    pub use crate::state::HandlerState;
}

use serde::{Deserialize, Serialize};
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
