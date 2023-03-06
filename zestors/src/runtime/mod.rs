/*!
# Overview

This module currently just exposes two functions:
- [`set_default_shutdown_time`] and [`get_default_shutdown_time`].

As supervision and distribution get implemented this module will fill further.

| __<--__ [`handler`](crate::handler) | [`supervision`](crate::supervision) __-->__ |
|---|---|
*/

use std::{
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
    time::Duration,
};

static DEFAULT_SHUTDOWN_TIME_NANOS: AtomicU32 = AtomicU32::new(0);
static DEFAULT_SHUTDOWN_TIME_SECS: AtomicU64 = AtomicU64::new(1);

/// Set the default shutdown time-limit.
/// 
/// This limit is the limit applied to a default [`Link`], and changes how long these processes
/// have to exit before 
pub fn set_default_shutdown_time(duration: Duration) {
    DEFAULT_SHUTDOWN_TIME_NANOS.store(duration.subsec_nanos(), Ordering::Release);
    DEFAULT_SHUTDOWN_TIME_SECS.store(duration.as_secs(), Ordering::Release);
}

/// Get the default shutdown time-limit.
pub fn get_default_shutdown_time() -> Duration {
    Duration::new(
        DEFAULT_SHUTDOWN_TIME_SECS.load(Ordering::Acquire),
        DEFAULT_SHUTDOWN_TIME_NANOS.load(Ordering::Acquire),
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
