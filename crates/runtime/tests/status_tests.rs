//! Tests for `ActorStatus`/`ExitStatus` (src/status.rs) and the status-derived
//! parts of `ActorOps` (src/ops.rs): `monitor`/`monitor_init`/`monitor_exit`,
//! `snapshot`, `uptime`, and the reference-count-aware `is_permanently_dead`.

use std::time::Duration;
use zestors_runtime::errors::ExitError;
use zestors_runtime::prelude::*;
use zestors_runtime::{ActorStatus, ExitStatus, StrongAddress, spawn_rand};

mod common;

// ============================================================================
// Pure enum logic - no actor required.
// ============================================================================

#[test]
fn actor_status_predicates_are_mutually_consistent() {
    let cases = [
        ActorStatus::Exited(ExitStatus::Normal),
        ActorStatus::Initializing,
        ActorStatus::Running,
        ActorStatus::Suspended,
        ActorStatus::Exiting,
    ];

    for status in cases {
        // `should_exit` is exactly Exiting or Exited.
        assert_eq!(
            status.should_exit(),
            matches!(status, ActorStatus::Exiting | ActorStatus::Exited(_))
        );
        // `accepts_messages` is exactly Initializing, Running or Suspended.
        assert_eq!(
            status.accepts_messages(),
            matches!(
                status,
                ActorStatus::Initializing | ActorStatus::Running | ActorStatus::Suspended
            )
        );
        // The five single-variant predicates partition the enum: exactly one
        // of them is true for any given status.
        let flags = [
            status.is_running(),
            status.is_suspended(),
            status.is_exiting(),
            status.is_dead(),
            status.is_initializing(),
        ];
        assert_eq!(
            flags.iter().filter(|f| **f).count(),
            1,
            "exactly one predicate should hold for {status:?}, got {flags:?}"
        );
    }
}

#[test]
fn exit_status_round_trips_through_result() {
    assert_eq!(ExitStatus::from_result(Ok(())), ExitStatus::Normal);
    assert!(ExitStatus::from_result(Ok(())).into_result().is_ok());

    for err in [
        ExitError::Panicked,
        ExitError::Aborted,
        ExitError::UnhandledError,
    ] {
        let status = ExitStatus::from_result(Err(err));
        assert_eq!(status.into_result(), Err(err));
        assert_eq!(ExitStatus::from(err), status);
    }
}

#[test]
fn exit_status_is_normal_xor_is_error() {
    for status in [
        ExitStatus::Normal,
        ExitStatus::Panicked,
        ExitStatus::Aborted,
        ExitStatus::UnhandledError,
    ] {
        assert_ne!(status.is_normal(), status.is_error());
        assert_eq!(status.is_normal(), status == ExitStatus::Normal);
    }
}

// ============================================================================
// Lifecycle transitions on a real actor.
// ============================================================================

#[tokio::test]
async fn full_lifecycle_transitions_in_order() {
    let child = spawn_rand(common::simplest_handler);

    // Spawning finishes its setup synchronously: the channel is already
    // `Initializing` the moment `spawn_rand` returns, before the task has
    // even had a chance to run.
    assert_eq!(child.status(), ActorStatus::Initializing);

    // The first `recv` inside `simplest_handler` completes initialization.
    child.monitor_init().await.unwrap();
    assert_eq!(child.status(), ActorStatus::Running);

    assert!(child.signal_suspend());
    assert!(common::wait_for_suspended(&child).await);
    assert_eq!(child.status(), ActorStatus::Suspended);

    assert!(child.signal_resume());
    assert!(common::wait_for_running(&child).await);
    assert_eq!(child.status(), ActorStatus::Running);

    assert!(child.signal_shutdown());
    // `Exiting` is transient (it may already have flipped to `Exited` by the
    // time we look), but it must never be skipped backwards into anything
    // else, so we only assert the terminal state here.
    child.monitor_exit().await.unwrap();
    assert_eq!(child.status(), ActorStatus::Exited(ExitStatus::Normal));
}

#[tokio::test]
async fn exit_status_reflects_panic() {
    let child: zestors_runtime::Child<(), ()> = spawn_rand(|mut inbox: Inbox<()>| async move {
        inbox.recv().await;
        panic!("intentional panic for exit-status test");
    });

    child.monitor_init().await.unwrap();
    let _ = child.cast(()).await;

    let outcome = child.monitor_exit().await;
    assert_eq!(outcome, Err(ExitError::Panicked));
    assert_eq!(child.status(), ActorStatus::Exited(ExitStatus::Panicked));
}

#[tokio::test]
async fn exit_status_reflects_unhandled_error() {
    let child: zestors_runtime::Child<(), ()> = spawn_rand(|mut inbox: Inbox<()>| async move {
        inbox.recv().await;
        Err(rootcause::report!("handler error"))
    });

    child.monitor_init().await.unwrap();
    let _ = child.cast(()).await;

    let outcome = child.monitor_exit().await;
    assert_eq!(outcome, Err(ExitError::UnhandledError));
    assert_eq!(
        child.status(),
        ActorStatus::Exited(ExitStatus::UnhandledError)
    );
}

#[tokio::test]
async fn monitor_init_reports_early_exit_instead_of_hanging() {
    let child: zestors_runtime::Child<(), ()> =
        spawn_rand(|_inbox: Inbox<()>| async { panic!("dies before first recv") });

    let outcome = child.monitor_init().await;
    assert_eq!(outcome, Err(ExitStatus::Panicked));
}

#[tokio::test]
async fn monitor_resolves_for_an_already_satisfied_condition() {
    let child = spawn_rand(common::simplest_handler);
    child.monitor_init().await.unwrap();

    // The status is already `Running`, so this must resolve without ever
    // observing a status change.
    let already_true = child
        .monitor(|status| status.is_running().then_some(()))
        .await;
    assert_eq!(already_true, ());

    child.signal_shutdown();
    child.monitor_exit().await.unwrap();
}

#[tokio::test]
async fn monitor_never_resolves_for_an_unreachable_condition() {
    let child = spawn_rand(common::simplest_handler);

    let timed_out = tokio::time::timeout(Duration::from_millis(100), child.monitor(|_| None::<()>))
        .await
        .is_err();
    assert!(timed_out);

    child.signal_shutdown();
}

// ============================================================================
// Reference-count-aware liveness (`is_dead` vs `is_permanently_dead`).
// ============================================================================

#[tokio::test]
async fn permanently_dead_requires_every_strong_ref_to_be_gone() {
    let child = spawn_rand(common::simplest_handler);
    child.monitor_init().await.unwrap();
    assert!(!child.is_permanently_dead());

    let strong = child
        .upgrade()
        .expect("actor is alive, upgrade must succeed");
    let weak = child.address().clone();

    child.signal_shutdown();
    child.monitor_exit().await.unwrap();

    // Dead, but `child` and `strong` both still hold a strong reference.
    assert!(weak.is_dead());
    assert!(!weak.is_permanently_dead());

    drop(child);
    assert!(
        !weak.is_permanently_dead(),
        "`strong` still holds one reference"
    );

    drop(strong);
    assert!(weak.is_permanently_dead(), "no strong references remain");
}

// ============================================================================
// Snapshot.
// ============================================================================

#[tokio::test]
async fn snapshot_reports_name_status_and_empty_queues() {
    let child = spawn_rand(common::simplest_handler);
    child.monitor_init().await.unwrap();

    let snapshot = child.snapshot();
    assert_eq!(&snapshot.name, child.name());
    assert_eq!(snapshot.status, ActorStatus::Running);
    assert_eq!(snapshot.msg_len, 0);
    assert_eq!(snapshot.signal_len, 0);
    assert_eq!(snapshot.spawns.len(), 1);
    assert!(snapshot.exits.is_empty());

    child.signal_shutdown();
    child.monitor_exit().await.unwrap();
}

#[tokio::test]
async fn snapshot_msg_len_reflects_unprocessed_messages() {
    // A handler that never calls `recv` at all, so every cast we perform is
    // guaranteed to still be sitting in the queue - no race with processing.
    let child = spawn_rand(|_inbox: Inbox<()>| async move {
        std::future::pending::<()>().await;
        Ok(())
    });

    for _ in 0..7 {
        child.try_cast(()).unwrap();
    }

    assert_eq!(child.snapshot().msg_len, 7);
    assert_eq!(child.msg_len(), 7);
    assert!(!child.msg_is_empty());

    child.abort();
}

#[tokio::test]
async fn spawn_and_exit_history_are_bounded_and_ordered() {
    let strong: StrongAddress<()> =
        StrongAddress::create(common::test_name("bounded_history")).unwrap();

    let mut last_child = None;
    for _ in 0..10 {
        let child = strong.clone().spawn(common::simplest_handler).unwrap();
        child.signal_shutdown();
        child.monitor_exit().await.unwrap();
        last_child = Some(child);
    }
    let child = last_child.unwrap();

    let spawns = child.spawned_at();
    let exits = child.snapshot().exits;

    // 10 respawns happened, but the history is bounded - and bounded to
    // something smaller than "keep everything".
    assert!(!spawns.is_empty());
    assert!(
        spawns.len() < 10,
        "spawn history should be capped, got {}",
        spawns.len()
    );
    assert!(!exits.is_empty());
    assert!(
        exits.len() < 10,
        "exit history should be capped, got {}",
        exits.len()
    );

    // Both histories are in chronological (oldest-first) order.
    assert!(spawns.windows(2).all(|w| w[0] <= w[1]));
    assert!(exits.windows(2).all(|w| w[0].0 <= w[1].0));

    assert_eq!(child.last_spawned_at(), spawns.last().copied());
}

#[tokio::test]
async fn uptime_counts_from_last_spawn_and_keeps_counting_after_exit() {
    let child = spawn_rand(common::simplest_handler);
    child.monitor_init().await.unwrap();

    let uptime_while_running = child.uptime().expect("spawned, so uptime is Some");

    child.signal_shutdown();
    child.monitor_exit().await.unwrap();

    let uptime_after_exit = child.uptime().expect("uptime keeps counting after exit");
    assert!(uptime_after_exit >= uptime_while_running);
}
