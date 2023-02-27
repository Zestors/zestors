use std::{
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
    time::Duration,
};

#[allow(unused)]
use crate::all::*;


/// The [`Link`] of an actor specifies how it should behave when the [`Child`] is dropped.
/// - If the link is [`Link::Detached`], then the actor will continue execution.
/// - If the link is [`Link::Attached(duration)`], then the actor will be shut-down with the
/// given [`ShutdownDuration`].
/// 
/// # Default
/// The default value for a link is `Link::Attached(ShutdownDuration::default())`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Link {
    Detached,
    Attached(ShutdownDuration),
}

impl Link {
    /// Set the link to [`Link::Attached(duration)`]. Returns the previous [`ShutdownDuration`] if
    /// it was attached before.
    pub fn attach(&mut self, mut duration: ShutdownDuration) -> Option<ShutdownDuration> {
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

    /// Set the link to [`Link::Detached`]. Returns the [`ShutdownDuration`] if it was attached before.
    pub fn detach(&mut self) -> Option<ShutdownDuration> {
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
        Link::Attached(ShutdownDuration::default())
    }
}

impl From<Duration> for Link {
    fn from(value: Duration) -> Self {
        Self::Attached(value.into())
    }
}

impl From<ShutdownDuration> for Link {
    fn from(value: ShutdownDuration) -> Self {
        Self::Attached(value)
    }
}

//------------------------------------------------------------------------------------------------
//  ShutdownDuration
//------------------------------------------------------------------------------------------------

/// A [`ShutdownDuration`] specifies how long an actor has to shut down before it is aborted.
/// The duration can be specified as:
/// - [`ShutdownDuration::Dynamic`] -> The duration will be returned at runtime with 
/// [`ShutdownDuration::get_default_duration()`].
/// - [`ShutdownDuration::Duration(duration)`] -> The duration given will be used.
/// 
/// The dynamic duration can be changed with [`ShutdownDuration::set_default_duration`].
/// 
/// # Default
/// The default value for a shutdown-duration is [`ShutdownDuration::Dynamic`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShutdownDuration {
    Dynamic,
    Duration(Duration),
}

static DEFAULT_ABORT_TIMER_NANOS: AtomicU32 = AtomicU32::new(0);
static DEFAULT_ABORT_TIMER_SECS: AtomicU64 = AtomicU64::new(1);

impl ShutdownDuration {
    /// Get the duration.
    pub fn get_duration(&self) -> Duration {
        match self {
            ShutdownDuration::Dynamic => Self::get_default_duration(),
            ShutdownDuration::Duration(duration) => duration.clone(),
        }
    }

    /// Into the duration.
    pub fn into_duration(self) -> Duration {
        match self {
            ShutdownDuration::Dynamic => Self::get_default_duration(),
            ShutdownDuration::Duration(duration) => duration,
        }
    }

    /// Set the default (dynamic) duration.
    pub fn set_default_duration(duration: Duration) {
        DEFAULT_ABORT_TIMER_NANOS.store(duration.subsec_nanos(), Ordering::Release);
        DEFAULT_ABORT_TIMER_SECS.store(duration.as_secs(), Ordering::Release);
    }

    /// Get the default (dynamic) duration.
    pub fn get_default_duration() -> Duration {
        Duration::new(
            DEFAULT_ABORT_TIMER_SECS.load(Ordering::Acquire),
            DEFAULT_ABORT_TIMER_NANOS.load(Ordering::Acquire),
        )
    }
}

impl Default for ShutdownDuration {
    fn default() -> Self {
        Self::Dynamic
    }
}

impl From<Duration> for ShutdownDuration {
    fn from(value: Duration) -> Self {
        Self::Duration(value)
    }
}

//------------------------------------------------------------------------------------------------
//  Default abort timer
//------------------------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn default_shutdown_time() {
        assert_eq!(ShutdownDuration::get_default_duration(), Duration::from_secs(1));
        ShutdownDuration::set_default_duration(Duration::from_secs(2));
        assert_eq!(ShutdownDuration::get_default_duration(), Duration::from_secs(2));
    }
}
