use super::*;
use std::sync::OnceLock;

static DEFAULT_BACKPRESSURE: OnceLock<BackPressure> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BackPressure {
    /// Queue occupancy at which backpressure starts, as a fraction [0, 1].
    starts_at: f32,

    /// Maximum delay applied at full capacity.
    max_delay: Duration,
}

impl BackPressure {
    pub const fn new(starts_at: f32, max_delay: Duration) -> Self {
        assert!(starts_at >= 0.0 && starts_at < 1.0);

        Self {
            starts_at,
            max_delay,
        }
    }

    pub fn delay(&self, len: usize, limit: usize) -> Option<Duration> {
        if limit == 0 || (len as f32 / limit as f32) < self.starts_at {
            return None;
        }

        let occupancy = len as f32 / limit as f32;

        let pressure = ((occupancy - self.starts_at) / (1.0 - self.starts_at)).clamp(0.0, 1.0);

        let pressure = pressure * pressure;

        Some(self.max_delay.mul_f32(pressure))
    }

    pub fn default() -> &'static Self {
        DEFAULT_BACKPRESSURE.get_or_init(|| Self::new(0.5, Duration::from_millis(10)))
    }
}

impl Default for BackPressure {
    fn default() -> Self {
        Self::new(0.75, Duration::from_millis(10))
    }
}
