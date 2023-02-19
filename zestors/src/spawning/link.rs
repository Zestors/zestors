use std::{
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
    time::Duration,
};

//------------------------------------------------------------------------------------------------
//  Link
//------------------------------------------------------------------------------------------------

/// This decides whether the actor is attached or detached. If it is attached, then the
/// abort-timer is specified here as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Link {
    Detached,
    Attached(ShutdownTime),
}

impl Link {
    pub fn attach(&mut self, mut time: ShutdownTime) -> Option<ShutdownTime> {
        match self {
            Link::Detached => {
                *self = Link::Attached(time);
                None
            }
            Link::Attached(old_time) => {
                std::mem::swap(old_time, &mut time);
                Some(time)
            }
        }
    }

    pub fn detach(&mut self) -> Option<ShutdownTime> {
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
        Link::Attached(ShutdownTime::default())
    }
}

impl From<Duration> for Link {
    fn from(value: Duration) -> Self {
        Self::Attached(value.into())
    }
}

impl From<ShutdownTime> for Link {
    fn from(value: ShutdownTime) -> Self {
        Self::Attached(value)
    }
}

//------------------------------------------------------------------------------------------------
//  ShutdownTime
//------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShutdownTime {
    Default,
    Duration(Duration),
}

static DEFAULT_ABORT_TIMER_NANOS: AtomicU32 = AtomicU32::new(0);
static DEFAULT_ABORT_TIMER_SECS: AtomicU64 = AtomicU64::new(1);

impl ShutdownTime {
    pub fn duration(&self) -> Duration {
        match self {
            ShutdownTime::Default => Self::get_default_duration(),
            ShutdownTime::Duration(duration) => duration.clone(),
        }
    }

    pub fn set_default_duration(duration: Duration) {
        DEFAULT_ABORT_TIMER_NANOS.store(duration.subsec_nanos(), Ordering::Release);
        DEFAULT_ABORT_TIMER_SECS.store(duration.as_secs(), Ordering::Release);
    }

    /// Get the current default abort-timer used for the default [Config].
    pub fn get_default_duration() -> Duration {
        Duration::new(
            DEFAULT_ABORT_TIMER_SECS.load(Ordering::Acquire),
            DEFAULT_ABORT_TIMER_NANOS.load(Ordering::Acquire),
        )
    }
}

impl Default for ShutdownTime {
    fn default() -> Self {
        Self::Default
    }
}

impl From<Duration> for ShutdownTime {
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
        assert_eq!(ShutdownTime::get_default_duration(), Duration::from_secs(1));
        ShutdownTime::set_default_duration(Duration::from_secs(2));
        assert_eq!(ShutdownTime::get_default_duration(), Duration::from_secs(2));
    }
}
