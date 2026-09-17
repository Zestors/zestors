use std::collections::VecDeque;
use tokio::time::Instant;
use zestors_supervision::RestartIntensity;

/// Controls how a [`Supervisor`](crate::Supervisor) reacts when one of its
/// children exits and
/// needs to be restarted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupervisionStrategy {
    /// Only the child that exited is restarted; its siblings are left alone.
    OneForOne,
    /// Every child is stopped and restarted together, whichever one exited.
    OneForAll,
    /// The child that exited, and every child started after it, are stopped
    /// (in reverse start order) and restarted together (in start order).
    RestForOne,
}

impl Default for SupervisionStrategy {
    fn default() -> Self {
        Self::OneForOne
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RestartLimiter {
    intensity: RestartIntensity,
    restarts: VecDeque<Instant>,
}

impl RestartLimiter {
    pub fn new(intensity: RestartIntensity) -> Self {
        Self {
            intensity,
            restarts: VecDeque::new(),
        }
    }

    pub fn acquire_permit(&mut self) -> bool {
        self.intensity.allow_restart(&mut self.restarts)
    }
}
