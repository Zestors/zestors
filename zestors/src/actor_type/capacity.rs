use std::time::Duration;

/// The configuration for spawning an inbox. This decides whether the inbox is bounded or unbounded. 
/// If it is unbounded then a [BackPressure] must be specified.
#[derive(Debug, Clone, PartialEq)]
pub enum Capacity {
    Bounded(usize),
    BackPressure(BackPressure),
    Unbounded
}

impl Capacity {
    /// Whether the capacity is bounded.
    pub fn is_bounded(&self) -> bool {
        matches!(self, Self::Bounded(_))
    }
}

impl Default for Capacity {
    fn default() -> Self {
        Capacity::BackPressure(BackPressure::default())
    }
}

/// The backpressure mechanism for unbounded inboxes.
#[derive(Debug, Clone, PartialEq)]
pub struct BackPressure {
    starts_at: usize,
    base_ns: u64,
    exp_growth: Option<f32>,
}

impl BackPressure {
    /// Creates a new linear backpressure.
    ///
    /// The timeout is calculated as follows:
    /// `timeout = timeout * (msg_count - start_at)`
    ///
    /// # Panics
    /// Panics if the `timeout` is bigger than `213_503 days`.
    pub fn linear(starts_at: usize, timeout: Duration) -> Self {
        let base_ns = timeout
            .as_nanos()
            .try_into()
            .expect("Base duration > 213_503 days");

        Self {
            starts_at,
            base_ns,
            exp_growth: None,
        }
    }

    /// Creates a new linear backpressure.
    ///
    /// The timeout is calculated as follows:
    /// `timeout = timeout * (factor ^ (msg_count - start_at))`
    ///
    /// # Panics
    /// Panics if the `factor` is negative, or if the `timeout` is bigger than `213_503 days.`
    pub fn exponential(starts_at: usize, timeout: Duration, factor: f32) -> Self {
        if factor < 0.0 {
            panic!("Negative factors not allowed!")
        }

        let base_ns = timeout
            .as_nanos()
            .try_into()
            .expect("Base duration > 213_503 days");

        Self {
            starts_at,
            base_ns,
            exp_growth: Some(factor),
        }
    }

    pub fn get_timeout(&self, msg_count: usize) -> Option<Duration> {
        if msg_count < self.starts_at {
            return None;
        }

        match self.exp_growth {
            Some(factor) => {
                let diff = (msg_count - self.starts_at).try_into().unwrap_or(i32::MAX);
                let mult = (factor as f64).powi(diff);
                let ns = self.base_ns as f64 * mult;
                Some(Duration::from_nanos(ns as u64))
            }
            None => {
                let diff = (msg_count - self.starts_at + 1) as u64;
                let ns = self.base_ns.saturating_mul(diff);
                Some(Duration::from_nanos(ns))
            }
        }
    }
}

impl Default for BackPressure {
    fn default() -> Self {
        Self::exponential(5, Duration::from_nanos(25), 1.3)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::time::Duration;

    #[test]
    fn backpressure_linear_start_at() {
        let bp = BackPressure::linear(10, Duration::from_secs(1));

        assert_eq!(bp.get_timeout(9), None);
        assert_eq!(bp.get_timeout(10), Some(Duration::from_secs(1)));
    }

    #[test]
    fn backpressure_exponential_start_at() {
        let bp = BackPressure::exponential(10, Duration::from_secs(1), 1.1);

        assert_eq!(bp.get_timeout(9), None);
        assert_eq!(
            bp.get_timeout(10),
            Some(Duration::from_nanos(1_000_000_000))
        );
    }

    #[test]
    fn backpressure_linear() {
        let bp = BackPressure::linear(0, Duration::from_secs(1));

        assert_eq!(bp.get_timeout(0), Some(Duration::from_secs(1)));
        assert_eq!(bp.get_timeout(1), Some(Duration::from_secs(2)));
        assert_eq!(bp.get_timeout(10), Some(Duration::from_secs(11)));
    }

    #[test]
    fn backpressure_exponential() {
        let bp = BackPressure::exponential(0, Duration::from_secs(1), 1.1);

        assert_eq!(bp.get_timeout(0), Some(Duration::from_nanos(1_000_000_000)));
        assert_eq!(bp.get_timeout(1), Some(Duration::from_nanos(1_100_000_023)));
        assert_eq!(bp.get_timeout(2), Some(Duration::from_nanos(1_210_000_052)));
        assert_eq!(bp.get_timeout(3), Some(Duration::from_nanos(1_331_000_086)));
    }

}