use crate::_prelude::*;
use std::collections::VecDeque;
use tokio::time::Instant;

/// How long a supervisor waits before restarting a child, growing
/// exponentially with every consecutive restart and capped at
/// [`RestartBackoff::max`].
///
/// The delay for the first restart is [`RestartBackoff::initial`]; every
/// further restart in the same failing streak multiplies the previous delay
/// by [`RestartBackoff::factor`], up to [`RestartBackoff::max`]. A child that
/// finishes initialization successfully resets the streak, so a genuinely
/// healthy child that crashes much later is restarted immediately (well,
/// after [`RestartBackoff::initial`]).
///
/// This is separate from [`RestartIntensity`]: the backoff slows down a
/// restart loop, while the intensity is what eventually stops it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RestartBackoff {
    /// The delay before the first restart in a failing streak.
    pub initial: Duration,

    /// The upper bound on the delay between two restarts.
    pub max: Duration,

    /// The factor by which the delay grows after every restart. Values below
    /// 2 make the backoff linear or constant; `1` means a constant
    /// [`RestartBackoff::initial`] delay.
    pub factor: u32,
}

impl Default for RestartBackoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(100),
            max: Duration::from_secs(30),
            factor: 2,
        }
    }
}

impl RestartBackoff {
    /// Creates a backoff that starts at `initial`, doubles, and is capped at
    /// `max`.
    pub fn new(initial: Duration, max: Duration) -> Self {
        Self {
            initial,
            max,
            factor: 2,
        }
    }

    /// Creates a [`RestartBackoff`] that never waits: every restart happens
    /// as soon as the supervisor observes the exit.
    pub fn none() -> Self {
        Self {
            initial: Duration::ZERO,
            max: Duration::ZERO,
            factor: 1,
        }
    }

    /// Sets [`RestartBackoff::initial`].
    pub fn initial(mut self, initial: Duration) -> Self {
        self.initial = initial;
        self
    }

    /// Sets [`RestartBackoff::max`].
    pub fn max(mut self, max: Duration) -> Self {
        self.max = max;
        self
    }

    /// Sets [`RestartBackoff::factor`].
    pub fn factor(mut self, factor: u32) -> Self {
        self.factor = factor;
        self
    }

    /// The delay before the restart after `consecutive_failures` earlier
    /// restarts in the current streak (0 for the first restart), capped at
    /// [`RestartBackoff::max`].
    pub fn delay(&self, consecutive_failures: u32) -> Duration {
        let max = self.max;
        let mut delay = self.initial.min(max);
        let mut remaining = consecutive_failures;

        while remaining > 0 && delay < max {
            let next = delay.saturating_mul(self.factor.max(1)).min(max);
            if next == delay {
                break;
            }
            delay = next;
            remaining -= 1;
        }

        delay
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_exponentially_up_to_the_cap() {
        let backoff = RestartBackoff::new(Duration::from_millis(100), Duration::from_millis(450));

        assert_eq!(backoff.delay(0), Duration::from_millis(100));
        assert_eq!(backoff.delay(1), Duration::from_millis(200));
        assert_eq!(backoff.delay(2), Duration::from_millis(400));
        assert_eq!(backoff.delay(3), Duration::from_millis(450));
        assert_eq!(backoff.delay(100), Duration::from_millis(450));
    }

    #[test]
    fn no_backoff_has_no_delay() {
        assert_eq!(RestartBackoff::none().delay(0), Duration::ZERO);
        assert_eq!(RestartBackoff::none().delay(50), Duration::ZERO);
    }

    #[test]
    fn zero_or_constant_factor_does_not_loop_forever() {
        let constant =
            RestartBackoff::new(Duration::from_millis(10), Duration::from_secs(1)).factor(1);
        assert_eq!(constant.delay(u32::MAX), Duration::from_millis(10));

        let zero = RestartBackoff::new(Duration::ZERO, Duration::from_secs(1)).factor(2);
        assert_eq!(zero.delay(u32::MAX), Duration::ZERO);
    }
}
