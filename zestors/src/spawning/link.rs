use std::time::Duration;

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

    pub fn into_duration_or_default(self) -> Duration {
        match self {
            Link::Detached => get_default_shutdown_time(),
            Link::Attached(duration) => duration,
        }
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
