use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug)]
pub(crate) struct RestartLimiter {
    limit: usize,
    within: Duration,
    values: Vec<Instant>,
    triggered: bool,
}

impl RestartLimiter {
    /// Create a new restart_limiter, with a given limit within the duration.
    pub fn new(limit: usize, within: Duration) -> Self {
        Self {
            limit,
            within,
            values: Vec::new(),
            triggered: false,
        }
    }

    /// Sets a new limit.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }

    /// Sets a new duration.
    pub fn set_within(&mut self, within: Duration) {
        self.within = within;
    }

    pub fn reset(&mut self) {
        self.triggered = false;
        self.values.drain(..);
    }

    /// Adds a restart and then checks whether the restart is within the limit.
    pub fn within_limit(&mut self) -> bool {
        if !self.triggered() {
            self.values.push(Instant::now());
            self.values
                .retain(|instant| instant.elapsed() < self.within);

            if self.values.len() <= self.limit {
                self.triggered = true
            }
        }

        self.triggered()
    }

    pub fn triggered(&self) -> bool {
        self.triggered
    }
}
