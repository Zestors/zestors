//! Tests for `spawn`/`spawn_rand`/`spawn_task`/`spawn_task_rand`
//! (src/spawn.rs): name handling, the four entry points, respawning on a
//! `StrongAddress`, and how panics/errors during spawn map to `ExitStatus`.

use zestors_runtime::errors::ConcurrentInboxError;
use zestors_runtime::prelude::*;
use zestors_runtime::{ActorStatus, StrongAddress, spawn, spawn_rand, spawn_task, spawn_task_rand};

mod common;

#[tokio::test]
async fn spawn_uses_the_given_name() {
    let name = common::test_name("spawn_specific_name");
    let child = spawn(name.clone(), common::simplest_handler).unwrap();

    assert_eq!(child.name(), &name);

    child.signal_shutdown();
}

#[tokio::test]
async fn spawn_rand_uses_a_fresh_random_name() {
    let child1 = spawn_rand(common::simplest_handler);
    let child2 = spawn_rand(common::simplest_handler);

    assert_ne!(child1.name(), child2.name());

    child1.signal_shutdown();
    child2.signal_shutdown();
}

#[tokio::test]
async fn spawn_task_cannot_receive_messages_but_can_receive_signals() {
    let child = spawn_task_rand(|mut task_box: zestors_runtime::TaskBox| async move {
        assert!(task_box.try_next().is_none());
        task_box.wait_shutdown().await;
        Ok::<_, rootcause::Report>("done")
    });
    child.monitor_init().await.unwrap();

    child.signal_shutdown();
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), child).await;
    assert_eq!(result.unwrap().unwrap(), "done");
}

#[tokio::test]
async fn spawn_task_with_a_specific_name_registers_it() {
    let name = common::test_name("spawn_task_specific");
    let child = spawn_task(name.clone(), |_task_box| async { Ok(()) }).unwrap();

    assert_eq!(child.name(), &name);
    assert!(zestors_runtime::Registry::local().contains(&name));
}

#[tokio::test]
async fn spawn_and_spawn_task_reject_a_duplicate_name() {
    let name = common::test_name("spawn_dup");
    let child = spawn(name.clone(), common::simplest_handler).unwrap();

    assert!(spawn(name.clone(), common::simplest_handler).is_err());
    assert!(spawn_task(name.clone(), |_task_box| async { Ok(()) }).is_err());

    child.signal_shutdown();
}

#[tokio::test]
async fn the_spawn_functions_return_the_handlers_own_result() {
    let child = spawn_rand(|mut inbox: Inbox<()>| async move {
        while inbox.recv().await.is_some() {}
        Ok::<_, rootcause::Report>(123)
    });
    child.signal_shutdown();
    assert_eq!(child.await.unwrap(), 123);

    let task = spawn_task_rand(|_task_box| async { Ok::<_, rootcause::Report>("hi") });
    assert_eq!(task.await.unwrap(), "hi");
}

// ============================================================================
// Respawning via `StrongAddress`.
// ============================================================================

#[tokio::test]
async fn spawn_on_a_strong_address_registers_the_name_it_was_created_with() {
    let name = common::test_name("strong_spawn_registers");
    let strong = StrongAddress::create(name.clone()).unwrap();
    let child = strong.spawn(common::simplest_handler).unwrap();

    assert_eq!(child.name(), &name);
    assert!(zestors_runtime::Registry::local().contains(&name));

    child.signal_shutdown();
}

#[tokio::test]
async fn respawning_reuses_the_same_channel_and_name() {
    let strong = StrongAddress::create(common::test_name("respawn_same_channel")).unwrap();
    let name = strong.name().clone();

    let child1 = strong.clone().spawn(common::simplest_handler).unwrap();
    child1.signal_shutdown();
    child1.monitor_exit().await.unwrap();

    let child2 = strong.spawn(common::simplest_handler).unwrap();
    assert_eq!(child2.name(), &name);
    assert_eq!(child2.status(), ActorStatus::Initializing);

    child2.signal_shutdown();
}

#[tokio::test]
async fn cannot_spawn_again_while_a_previous_run_is_still_active() {
    let strong = StrongAddress::create(common::test_name("respawn_while_active")).unwrap();
    let child = strong.clone().spawn(common::simplest_handler).unwrap();

    let result = strong.spawn(common::simplest_handler);
    assert!(matches!(result, Err(ConcurrentInboxError)));

    child.signal_shutdown();
}

#[tokio::test]
async fn respawning_with_a_different_handler_is_allowed() {
    let strong = StrongAddress::create(common::test_name("respawn_different_handler")).unwrap();

    let child1: zestors_runtime::Child<i32, ()> = strong
        .clone()
        .spawn(|mut inbox: Inbox<()>| async move {
            while inbox.recv().await.is_some() {}
            Ok::<_, rootcause::Report>(1)
        })
        .unwrap();
    child1.signal_shutdown();
    assert_eq!(child1.await.unwrap(), 1);

    let child2: zestors_runtime::Child<i32, ()> = strong
        .spawn(|mut inbox: Inbox<()>| async move {
            while inbox.recv().await.is_some() {}
            Ok::<_, rootcause::Report>(2)
        })
        .unwrap();
    child2.signal_shutdown();
    assert_eq!(child2.await.unwrap(), 2);
}

// ============================================================================
// Panics and errors surfacing through spawn.
// ============================================================================

#[tokio::test]
async fn a_panic_before_the_first_recv_still_registers_spawned_and_exited() {
    let strong = StrongAddress::create(common::test_name("panic_before_recv")).unwrap();
    let child: zestors_runtime::Child<(), ()> = strong
        .clone()
        .spawn(|_inbox: Inbox<()>| async { panic!("boom") })
        .unwrap();

    let result = tokio::time::timeout(std::time::Duration::from_secs(2), child).await;
    assert!(result.is_ok());
    assert_eq!(
        strong.status(),
        ActorStatus::Exited(zestors_runtime::ExitStatus::Panicked)
    );
    assert_eq!(strong.spawned_at().len(), 1);
}

#[tokio::test]
async fn spawn_error_result_maps_to_unhandled_error_exit_status() {
    let child: zestors_runtime::Child<(), ()> = spawn_rand(|mut inbox: Inbox<()>| async move {
        inbox.recv().await;
        Err(rootcause::report!("spawn function failed"))
    });
    child.monitor_init().await.unwrap();
    let _ = child.cast(()).await;

    let outcome = child.monitor_exit().await;
    assert_eq!(
        outcome,
        Err(zestors_runtime::errors::ExitError::UnhandledError)
    );
}

// ============================================================================
// Timestamps recorded at spawn time.
// ============================================================================

#[tokio::test]
async fn created_at_is_stable_across_respawns_while_last_spawned_at_updates() {
    let strong = StrongAddress::create(common::test_name("created_vs_spawned")).unwrap();
    let created_at = strong.created_at();

    let child1 = strong.clone().spawn(common::simplest_handler).unwrap();
    let first_spawned_at = child1.last_spawned_at().unwrap();
    assert_eq!(child1.created_at(), created_at);
    child1.signal_shutdown();
    child1.monitor_exit().await.unwrap();

    let child2 = strong.spawn(common::simplest_handler).unwrap();
    let second_spawned_at = child2.last_spawned_at().unwrap();

    assert_eq!(
        child2.created_at(),
        created_at,
        "created_at is fixed for the channel's lifetime"
    );
    assert!(second_spawned_at >= first_spawned_at);

    child2.signal_shutdown();
}
