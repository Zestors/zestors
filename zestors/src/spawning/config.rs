//------------------------------------------------------------------------------------------------
//  Link
//------------------------------------------------------------------------------------------------

use std::{time::Duration, sync::atomic::{AtomicU32, AtomicU64, Ordering}};

/// This decides whether the actor is attached or detached. If it is attached, then the
/// abort-timer is specified here as well.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Link {
    Detached,
    Attached(Duration),
}

impl Link {
    pub fn attach(&mut self, mut duration: Duration) -> Option<Duration> {
        match self {
            Link::Detached => {
                *self = Link::Attached(duration);
                None
            }
            Link::Attached(old_duration) => {
                std::mem::swap(old_duration, &mut duration);
                Some(duration)
            }
        }
    }

    pub fn detach(&mut self) -> Option<Duration> {
        match self {
            Link::Detached => {
                *self = Link::Detached;
                None
            }
            Link::Attached(_) => {
                let mut link = Link::Detached;
                std::mem::swap(self, &mut link);
                match link {
                    Link::Attached(duration) => Some(duration),
                    Link::Detached => unreachable!(),
                }
            }
        }
    }

    /// Whether the link is attached.
    pub fn is_attached(&self) -> bool {
        matches!(self, Link::Attached(_))
    }
}

impl Default for Link {
    fn default() -> Self {
        Link::Attached(get_default_shutdown_time())
    }
}

//------------------------------------------------------------------------------------------------
//  Default abort timer
//------------------------------------------------------------------------------------------------

static DEFAULT_ABORT_TIMER_NANOS: AtomicU32 = AtomicU32::new(0);
static DEFAULT_ABORT_TIMER_SECS: AtomicU64 = AtomicU64::new(1);

/// Set the abort-timer used for default spawn [Config].
///
/// This is applied globally for processes spawned after setting this value.
pub fn set_default_shutdown_time(timer: Duration) {
    DEFAULT_ABORT_TIMER_NANOS.store(timer.subsec_nanos(), Ordering::Release);
    DEFAULT_ABORT_TIMER_SECS.store(timer.as_secs(), Ordering::Release);
}

/// Get the current default abort-timer used for the default [Config].
pub fn get_default_shutdown_time() -> Duration {
    Duration::new(
        DEFAULT_ABORT_TIMER_SECS.load(Ordering::Acquire),
        DEFAULT_ABORT_TIMER_NANOS.load(Ordering::Acquire),
    )
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn default_shutdown_time() {
        assert_eq!(get_default_shutdown_time(), Duration::from_secs(1));
        set_default_shutdown_time(Duration::from_secs(2));
        assert_eq!(get_default_shutdown_time(), Duration::from_secs(2));
    }
}
