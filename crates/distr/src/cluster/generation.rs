//! Choosing a node's generation: a number that grows with every restart, so
//! that a restarted node replaces its previous incarnation instead of being
//! taken for an outdated one.
//!
//! The wall clock alone is not enough: a clock that steps back between two runs
//! would give the new run a lower generation than the old one, which peers that
//! remember the old one then ignore. So the generation is the clock, but never
//! at or below one already handed out, in this process or (with a store file)
//! in an earlier one.

use std::{
    io,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// The last generation handed out by this process.
static LAST: AtomicU64 = AtomicU64::new(0);

/// A generation for a node starting now, higher than any earlier one from this
/// process and, if `store` is given, than the one recorded there. The result is
/// recorded in `store` before it is returned.
pub(super) fn next(store: Option<&Path>) -> io::Result<u64> {
    let stored = match store {
        Some(path) => read(path)?,
        None => 0,
    };

    let mut last = LAST.load(Ordering::Relaxed);
    let generation = loop {
        let generation = pick(now_millis(), last.max(stored));
        match LAST.compare_exchange(last, generation, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break generation,
            Err(current) => last = current,
        }
    };

    if let Some(path) = store {
        write(path, generation)?;
    }
    Ok(generation)
}

/// The current time, unless that isn't past `floor`.
fn pick(now: u64, floor: u64) -> u64 {
    now.max(floor.saturating_add(1))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The generation recorded in `path`; 0 if there is none yet. A file that
/// can't be understood is an error rather than a reset, since a reset could
/// hand out a generation that peers already know to be outdated.
fn read(path: &Path) -> io::Result<u64> {
    match std::fs::read_to_string(path) {
        Ok(text) => text.trim().parse().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} does not hold a generation", path.display()),
            )
        }),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(err) => Err(err),
    }
}

/// Replaces the contents of `path` in one step, so a crash never leaves a
/// half-written generation behind.
fn write(path: &Path, generation: u64) -> io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    std::fs::write(&temp, generation.to_string())?;
    std::fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_clock_when_it_is_ahead() {
        assert_eq!(pick(1_000, 10), 1_000);
    }

    #[test]
    fn never_goes_back_when_the_clock_does() {
        assert_eq!(pick(100, 500), 501);
        assert_eq!(pick(500, 500), 501);
    }

    fn store(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("zestors-{name}-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn grows_across_calls_in_a_process() {
        let (a, b) = (next(None).unwrap(), next(None).unwrap());
        assert!(b > a);
    }

    #[test]
    fn grows_past_what_an_earlier_run_recorded() {
        let path = store("ahead");
        // An earlier run whose clock was far ahead of this one's.
        let far_ahead = now_millis() + 3_600_000;
        write(&path, far_ahead).unwrap();

        let generation = next(Some(&path)).unwrap();
        assert!(generation > far_ahead);
        assert_eq!(read(&path).unwrap(), generation);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_store_starts_fresh_and_a_garbled_one_is_refused() {
        let path = store("garbled");
        assert!(next(Some(&path)).is_ok());

        std::fs::write(&path, "not a number").unwrap();
        assert_eq!(
            next(Some(&path)).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let _ = std::fs::remove_file(&path);
    }
}
