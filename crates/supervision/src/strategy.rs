use crate::_prelude::*;
use std::collections::VecDeque;
use tokio::time::Instant;

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
