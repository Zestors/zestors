use crate::_prelude::*;
use rootcause::Report;
use std::collections::VecDeque;
use tokio::time::Instant;
use zestors_runtime::registry::RegistryAddError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RestartIntensity {
    max_restarts: usize,
    within: Duration,
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
    pub(super) fn allow_restart(&self, restarts: &mut VecDeque<Instant>) -> bool {
        let now = Instant::now();

        while let Some(front) = restarts.front() {
            if now.duration_since(*front) > self.within {
                restarts.pop_front();
            } else {
                break;
            }
        }

        if restarts.len() >= self.max_restarts {
            return false;
        }

        restarts.push_back(now);
        true
    }

    pub fn new(max_restarts: usize, within: Duration) -> Self {
        Self {
            max_restarts,
            within,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupervisionStrategy {
    OneForOne,
    OneForAll,
    RestForOne,
}

impl Default for SupervisionStrategy {
    fn default() -> Self {
        Self::OneForOne
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Terminal child error: {id}, error: {error:?}")]
pub struct RestartLimitReached {
    pub id: Pid,
    pub error: Option<Report>,
}

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("Restart limit reached for child {0}")]
    RestartLimit(#[from] RestartLimitReached),

    #[error("Another process is already registered with the same pid")]
    RegistryAddError(
        #[source]
        #[from]
        RegistryAddError,
    ),
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

    pub fn allow_restart(&mut self) -> bool {
        self.intensity.allow_restart(&mut self.restarts)
    }
}
