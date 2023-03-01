use std::{
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
    time::Duration,
};

#[allow(unused)]
use crate::all::*;

/// The [`Link`] of an actor specifies how it should behave when the [`Child`] is dropped.
/// - If the link is [`Link::Detached`], then the actor will continue execution.
/// - If the link is [`Link::Attached(Duration)`], then the actor will be shut-down with the
/// given duration.
///
/// # Default
/// The default value for a link is `Link::Attached(get_default_shutdown_time)`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Link {
    Detached,
    Attached(Duration),
}

impl Link {
    /// Set the link to [`Link::Attached(duration)`]. Returns the previous shutdown-time if
    /// it was attached before.
    pub fn attach(&mut self, mut duration: Duration) -> Option<Duration> {
        match self {
            Link::Detached => {
                *self = Link::Attached(duration);
                None
            }
            Link::Attached(old_time) => {
                std::mem::swap(old_time, &mut duration);
                Some(duration)
            }
        }
    }

    /// Set the link to [`Link::Detached`]. Returns the shutdown-time if it was attached before.
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
                    Link::Attached(time) => Some(time),
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

impl From<Duration> for Link {
    fn from(value: Duration) -> Self {
        Self::Attached(value.into())
    }
}

//------------------------------------------------------------------------------------------------
//  Shutdown-time
//------------------------------------------------------------------------------------------------

static DEFAULT_SHUTDOWN_TIME_NANOS: AtomicU32 = AtomicU32::new(0);
static DEFAULT_SHUTDOWN_TIME_SECS: AtomicU64 = AtomicU64::new(1);

/// Set the default shutdown-time limit.
/// This only changes the time for processes that have not been spawned yet.
pub fn set_default_shutdown_time(duration: Duration) {
    DEFAULT_SHUTDOWN_TIME_NANOS.store(duration.subsec_nanos(), Ordering::Release);
    DEFAULT_SHUTDOWN_TIME_SECS.store(duration.as_secs(), Ordering::Release);
}

/// Get the default shutdown-time limit.
pub fn get_default_shutdown_time() -> Duration {
    Duration::new(
        DEFAULT_SHUTDOWN_TIME_SECS.load(Ordering::Acquire),
        DEFAULT_SHUTDOWN_TIME_NANOS.load(Ordering::Acquire),
    )
}

//------------------------------------------------------------------------------------------------
//  Default abort timer
//------------------------------------------------------------------------------------------------

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
