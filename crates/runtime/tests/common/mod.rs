//! Shared helpers for the runtime integration tests.
//!
//! Each test file compiles this module as part of its own separate binary
//! and only uses a subset of it, so per-binary dead-code warnings here are
//! expected noise rather than a signal - hence the blanket `allow` below.
#![allow(dead_code)]
//!
//! Keep this deliberately small. Most tests can drive an actor to a known
//! state with the crate's own deterministic primitives (`watch_init`,
//! `watch_exit`, `ping`, or a `call` that acts as a barrier against
//! previously-cast messages — see the FIFO ordering guarantee documented on
//! [`zestors_runtime::Channel`]'s single per-actor queue). Reach for
//! `wait_for_*` only when there's genuinely no such signal to wait on; it
//! exists to fail fast with a clear timeout, not to paper over a race with a
//! guessed sleep duration.

use rootcause::Report;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::time::timeout;
use zestors_runtime::prelude::*;
use zestors_runtime::{ActorRef, ActorStatus};

/// An actor body that only ever consumes and discards messages, exiting
/// once the inbox is closed (i.e. after a [`Signal::Shutdown`] has drained
/// the queue).
pub async fn simplest_handler(mut inbox: Inbox<()>) -> Result<(), Report> {
    while inbox.recv().await.is_some() {}
    Ok(())
}

/// A stable [`Name`] for tests that want a human-readable, collision-free
/// name rather than [`Name::rand`].
pub fn test_name(name: &str) -> Name {
    Name::new(format!("test_{name}"))
}

/// Waits, with a generous safety-net timeout, for `actor`'s status to
/// satisfy `check_for`. The timeout only guards against the test hanging
/// forever if the condition is never met - it is not a substitute for a
/// deterministic wait.
async fn wait_for<T: ActorRef>(
    actor: &T,
    check_for: impl FnMut(ActorStatus) -> Option<()> + Send + 'static,
) -> bool {
    timeout(Duration::from_secs(5), actor.watch(check_for))
        .await
        .is_ok()
}

pub async fn wait_for_running<T: ActorRef>(actor: &T) -> bool {
    wait_for(actor, |s| s.is_running().then_some(())).await
}

pub async fn wait_for_suspended<T: ActorRef>(actor: &T) -> bool {
    wait_for(actor, |s| s.is_suspended().then_some(())).await
}

pub async fn wait_for_exiting<T: ActorRef>(actor: &T) -> bool {
    wait_for(actor, |s| s.is_exiting().then_some(())).await
}

pub async fn wait_for_dead<T: ActorRef>(actor: &T) -> bool {
    wait_for(actor, |s| s.is_dead().then_some(())).await
}

/// A simple `Send + Sync` counter for handlers that need to report how many
/// times something happened back to the test.
#[derive(Clone, Default)]
pub struct MessageCounter(Arc<AtomicUsize>);

impl MessageCounter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn increment(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    pub fn get(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}
