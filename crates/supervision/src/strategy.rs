use crate::_prelude::*;
use std::collections::VecDeque;
use tokio::time::Instant;

/// Controls how a [`Supervisor`] reacts when one of its children exits and
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
