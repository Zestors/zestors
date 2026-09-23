//! Tests for the reference types (src/references/*): `Address`,
//! `StrongAddress`, `Inbox`, `Child`, `TaskBox`, and the `IntoDyn`/`AsDyn`
//! conversions between statically- and dynamically-typed references.

use std::time::Duration;
use zestors_interface::{Envelope, Interface, Message};
use zestors_runtime::errors::{Cancelled, ConcurrentInboxError, JoinError, ShutdownAbortError};
use zestors_runtime::prelude::*;
use zestors_runtime::{ActorStatus, Dyn, StrongAddress, spawn, spawn_rand, spawn_task_rand};

mod common;

#[derive(Message, Debug, Clone)]
#[zestors(interface_path = "zestors_interface")]
struct Ping;

#[derive(Message, Debug, Clone)]
#[zestors(interface_path = "zestors_interface")]
struct Pong;

#[derive(Interface, Debug)]
#[zestors(interface_path = "zestors_interface")]
enum PingPongInterface {
    Ping(Envelope<Ping>),
    Pong(Envelope<Pong>),
}

#[derive(Interface, Debug)]
#[zestors(interface_path = "zestors_interface")]
enum UnrelatedInterface {
    Ping(Envelope<Ping>),
}

async fn ping_pong_handler(mut inbox: Inbox<PingPongInterface>) -> Result<(), rootcause::Report> {
    while inbox.recv().await.is_some() {}
    Ok(())
}

// ============================================================================
// Address: a weak reference that never keeps the channel alive.
// ============================================================================

#[tokio::test]
async fn address_does_not_count_toward_strong_count() {
    let child = spawn_rand(common::simplest_handler);

    let strong_before = child.strong_count();
    let weak_before = child.weak_count();
    let _addr1 = child.address().clone();
    let _addr2 = child.address().clone();
    assert_eq!(
        child.strong_count(),
        strong_before,
        "cloning an Address must not change strong_count"
    );
    assert_eq!(child.weak_count(), weak_before + 2);

    child.signal_shutdown();
}

#[tokio::test]
async fn address_can_still_send_after_every_strong_ref_is_dropped() {
    let name = common::test_name("weak_after_strong_gone");
    let child = spawn(name, common::simplest_handler).unwrap();
    let weak = child.address().clone();

    child.signal_shutdown();
    child.monitor_exit().await.unwrap();
    drop(child);

    assert!(weak.is_permanently_dead());
    // Sending to a permanently dead channel fails, it doesn't panic or hang.
    assert!(weak.cast(()).await.is_err());
}

// ============================================================================
// StrongAddress: keeps the channel alive and allows respawning.
// ============================================================================

#[tokio::test]
async fn strong_address_create_registers_without_spawning() {
    let name = common::test_name("strong_no_spawn");
    let strong: StrongAddress<()> = StrongAddress::create(name.clone()).unwrap();

    assert!(zestors_runtime::Registry::local().contains(&name));
    // A freshly created channel with nothing spawned on it starts `Exited`.
    assert_eq!(
        strong.status(),
        ActorStatus::Exited(zestors_runtime::ExitStatus::Normal)
    );

    drop(strong);
    assert!(!zestors_runtime::Registry::local().contains(&name));
}

#[tokio::test]
async fn spawning_twice_on_the_same_strong_address_is_rejected() {
    let strong = StrongAddress::create(common::test_name("concurrent_inbox")).unwrap();
    let child = strong.clone().spawn(common::simplest_handler).unwrap();

    let result = strong.spawn(common::simplest_handler);
    assert!(matches!(result, Err(ConcurrentInboxError)));

    child.signal_shutdown();
}

#[tokio::test]
async fn respawn_reuses_the_name_and_registry_entry() {
    let strong = StrongAddress::create(common::test_name("respawn_reuse")).unwrap();
    let name = strong.name().clone();

    let child1 = strong.clone().spawn(common::simplest_handler).unwrap();
    assert_eq!(child1.name(), &name);
    child1.signal_shutdown();
    child1.monitor_exit().await.unwrap();

    let child2 = strong.spawn(common::simplest_handler).unwrap();
    assert_eq!(child2.name(), &name);
    assert!(zestors_runtime::Registry::local().contains(&name));

    child2.signal_shutdown();
}

#[tokio::test]
async fn upgrade_fails_once_permanently_dead_and_succeeds_otherwise() {
    let child = spawn_rand(common::simplest_handler);
    child.monitor_init().await.unwrap();

    let address = child.address().clone();
    assert!(address.upgrade().is_some());

    child.signal_shutdown();
    child.monitor_exit().await.unwrap();
    drop(child);

    assert!(address.is_permanently_dead());
    assert!(address.upgrade().is_none());
}

// ============================================================================
// Inbox: strong reference, initialization modes, and drain-on-drop.
// ============================================================================

#[tokio::test]
async fn manual_init_suppresses_the_automatic_running_transition() {
    let proceed = std::sync::Arc::new(tokio::sync::Notify::new());
    let proceed2 = proceed.clone();

    let child = spawn_rand(move |mut inbox: Inbox<()>| async move {
        inbox.set_manual_init();
        proceed2.notified().await; // wait for the test to tell us to proceed
        assert!(
            inbox.register_initialized(),
            "the first call must still perform the transition"
        );
        while inbox.recv().await.is_some() {}
        Ok(())
    });

    // Give the task a chance to run up to `notified().await` and register
    // manual init, without ever calling a `recv*` method.
    tokio::task::yield_now().await;
    assert_eq!(
        child.status(),
        ActorStatus::Initializing,
        "manual init must suppress the automatic transition"
    );

    proceed.notify_one();
    assert!(common::wait_for_running(&child).await);

    child.signal_shutdown();
    child.monitor_exit().await.unwrap();
}

#[tokio::test]
async fn register_initialized_is_idempotent() {
    let child = spawn_rand(|mut inbox: Inbox<()>| async move {
        // First call actually transitions and reports `true`.
        assert!(inbox.register_initialized());
        // Every subsequent call is a no-op reporting `false`.
        assert!(!inbox.register_initialized());
        while inbox.recv().await.is_some() {}
        Ok(())
    });

    child.monitor_init().await.unwrap();
    child.signal_shutdown();
    child.monitor_exit().await.unwrap();
}

#[tokio::test]
async fn try_recv_finds_a_message_queued_before_the_actor_started_polling() {
    let child = spawn_rand(|mut inbox: Inbox<()>| async move {
        let first = inbox.try_recv();
        assert!(matches!(first, Some(InboxEvent::Message(()))));
        assert!(
            inbox.try_recv().is_none(),
            "nothing should be left after that"
        );
        Ok(())
    });

    // Spawning already leaves the channel `Initializing` synchronously
    // (before the task is even scheduled), so this succeeds despite the
    // task not having run at all yet - by the time it does, the message is
    // already sitting in the queue for `try_recv` to find immediately.
    child.try_cast(()).unwrap();

    let result = tokio::time::timeout(Duration::from_secs(2), child).await;
    assert!(result.unwrap().is_ok());
}

#[tokio::test]
async fn recv_event_stops_but_recv_event_always_keeps_waiting_once_exiting_and_empty() {
    // Once `Shutdown` has been popped and the message queue is empty,
    // `next_event`'s guard makes plain `recv_event` (`while_exiting: false`)
    // return `None` right away rather than waiting for anything further.
    let plain = spawn_rand(|mut inbox: Inbox<()>| async move {
        assert!(matches!(
            inbox.recv_event().await,
            Some(InboxEvent::Signal(Signal::Shutdown))
        ));
        assert!(inbox.recv_event().await.is_none());
        Ok(())
    });
    plain.monitor_init().await.unwrap();
    plain.signal_shutdown();
    assert!(
        tokio::time::timeout(Duration::from_secs(2), plain)
            .await
            .is_ok()
    );

    // `recv_event_always` (`while_exiting: true`) skips that guard, so in
    // the exact same situation it must keep waiting instead of resolving.
    let always = spawn_rand(|mut inbox: Inbox<()>| async move {
        assert!(matches!(
            inbox.recv_event_always().await,
            Some(InboxEvent::Signal(Signal::Shutdown))
        ));
        let second =
            tokio::time::timeout(Duration::from_millis(200), inbox.recv_event_always()).await;
        assert!(
            second.is_err(),
            "recv_event_always must not give up just because the queue emptied out while exiting"
        );
        Ok(())
    });
    always.monitor_init().await.unwrap();
    always.signal_shutdown();
    assert!(
        tokio::time::timeout(Duration::from_secs(3), always)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn dropping_the_inbox_drains_its_queue() {
    // A handler that receives exactly one message and then returns without
    // consuming the rest, so its `Inbox` gets dropped with a non-empty
    // queue. `Inbox::drop` must drain it rather than leaving it stuck.
    let child = spawn_rand(|mut inbox: Inbox<()>| async move {
        inbox.recv().await;
        Ok(())
    });
    child.monitor_init().await.unwrap();

    for _ in 0..5 {
        child.cast(()).await.unwrap();
    }

    let outcome = tokio::time::timeout(Duration::from_secs(2), child.monitor_exit()).await;
    assert!(
        outcome.is_ok(),
        "the actor should exit promptly even with unread messages left"
    );
}

// ============================================================================
// TaskBox: signal-only actors.
// ============================================================================

#[tokio::test]
async fn task_box_wait_shutdown_returns_once_shutdown_is_seen() {
    let child = spawn_task_rand(|mut task_box: zestors_runtime::TaskBox| async move {
        task_box.wait_shutdown().await;
        Ok::<_, rootcause::Report>("done")
    });

    child.monitor_init().await.unwrap();
    child.signal_shutdown();

    let result = tokio::time::timeout(Duration::from_secs(2), child).await;
    assert_eq!(result.unwrap().unwrap(), "done");
}

#[tokio::test]
async fn task_box_wait_shutdown_is_immediate_if_already_exiting() {
    let child = spawn_task_rand(|mut task_box: zestors_runtime::TaskBox| async move {
        // The first call actually waits for and consumes the `Shutdown`
        // signal, which leaves the channel `Exiting`.
        task_box.wait_shutdown().await;
        // The second call must take the `is_exiting()` early-return path
        // instead of waiting for another signal that will never come.
        task_box.wait_shutdown().await;
        Ok(())
    });
    child.monitor_init().await.unwrap();
    child.signal_shutdown();

    let result = tokio::time::timeout(Duration::from_secs(2), child).await;
    assert!(result.is_ok());
}

// ============================================================================
// `run_until_shutdown`: cooperative cancellation.
// ============================================================================

#[tokio::test]
async fn run_until_shutdown_completes_normally_without_a_signal() {
    let child = spawn_rand(|mut inbox: Inbox<()>| async move {
        let outcome = inbox.run_until_shutdown(async { 42 }).await;
        assert!(matches!(outcome, Ok(42)));
        Ok(())
    });

    let result = tokio::time::timeout(Duration::from_secs(2), child).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn run_until_shutdown_cancels_a_pending_future_on_shutdown() {
    let child = spawn_rand(|mut inbox: Inbox<()>| async move {
        let outcome = inbox.run_until_shutdown(std::future::pending::<()>()).await;
        assert!(matches!(outcome, Err(Cancelled)));
        Ok(())
    });

    child.monitor_init().await.unwrap();
    child.signal_shutdown();

    let result = tokio::time::timeout(Duration::from_secs(2), child).await;
    assert!(
        result.is_ok(),
        "run_until_shutdown should have unblocked the actor promptly"
    );
}

#[tokio::test]
async fn run_until_shutdown_is_cancelled_immediately_if_already_exiting() {
    let child = spawn_rand(|mut inbox: Inbox<()>| async move {
        inbox.register_initialized();
        inbox.register_exiting();
        // If this awaited `fut` at all, the test would hang - it must
        // short-circuit to `Cancelled` before ever polling it.
        let outcome = inbox.run_until_shutdown(std::future::pending::<()>()).await;
        assert!(matches!(outcome, Err(Cancelled)));
        Ok(())
    });

    let result = tokio::time::timeout(Duration::from_secs(2), child).await;
    assert!(result.is_ok());
}

// ============================================================================
// Child: handle semantics (abort/detach/into_*).
// ============================================================================

#[tokio::test]
async fn dropping_an_attached_child_aborts_it() {
    let name = common::test_name("attached_drop_aborts");
    let child = spawn(name.clone(), |mut inbox: Inbox<()>| async move {
        std::future::pending::<()>().await;
        while inbox.recv().await.is_some() {}
        Ok(())
    })
    .unwrap();
    // This handler blocks on `pending()` before ever touching the inbox, so
    // it never reaches `Running` - only `monitor_exit`-style waits make sense
    // here, not `monitor_init`.
    tokio::task::yield_now().await;

    let weak = child.address().clone();
    drop(child);

    let outcome = tokio::time::timeout(Duration::from_secs(2), weak.monitor_exit()).await;
    assert!(
        outcome.is_ok(),
        "dropping an attached Child should eventually abort its task"
    );
    assert_eq!(
        weak.status(),
        ActorStatus::Exited(zestors_runtime::ExitStatus::Aborted)
    );
}

#[tokio::test]
async fn detach_prevents_abort_on_drop() {
    let child = spawn_rand(|mut inbox: Inbox<()>| async move {
        // Stays alive only as long as the (soon-to-be-dropped) Child would
        // have kept it running for, if it weren't detached.
        tokio::time::sleep(Duration::from_millis(50)).await;
        while inbox.recv().await.is_some() {}
        Ok(())
    });
    child.monitor_init().await.unwrap();

    let mut child = child;
    child.detach();
    assert!(!child.is_attached());

    let weak = child.address().clone();
    drop(child);

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !weak.is_dead(),
        "a detached Child must not abort its task on drop"
    );

    weak.signal_shutdown();
}

#[tokio::test]
async fn abort_marks_the_exit_status_as_aborted() {
    let mut child = spawn_rand(|mut inbox: Inbox<()>| async move {
        std::future::pending::<()>().await;
        while inbox.recv().await.is_some() {}
        Ok(())
    });
    // Never reaches `Running` (it blocks before ever touching the inbox),
    // but the task must still be scheduled at least once before we abort it.
    tokio::task::yield_now().await;
    child.abort();
    let result = (&mut child).await;
    assert!(matches!(result, Err(JoinError::Aborted)));
    assert_eq!(
        child.status(),
        ActorStatus::Exited(zestors_runtime::ExitStatus::Aborted)
    );
}

#[tokio::test]
async fn shutdown_abort_gives_a_slow_actor_a_grace_period_then_aborts() {
    let child = spawn_rand(|mut inbox: Inbox<()>| async move {
        // Ignores shutdown signals entirely, so it can only ever be
        // stopped by an abort.
        std::future::pending::<()>().await;
        while inbox.recv().await.is_some() {}
        Ok(())
    });
    tokio::task::yield_now().await;

    let result = child.shutdown_abort(Duration::from_millis(100)).await;
    match result {
        Err(ShutdownAbortError { aborted, .. }) => assert!(aborted),
        Ok(_) => panic!("actor never yields to shutdown, so this must have been aborted"),
    }
}

#[tokio::test]
async fn shutdown_abort_returns_ok_for_a_cooperative_actor() {
    let child = spawn_rand(common::simplest_handler);
    child.monitor_init().await.unwrap();

    let result = child.shutdown_abort(Duration::from_secs(2)).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn into_parts_and_into_handle_hand_back_ownership() {
    let child = spawn_rand(common::simplest_handler);
    child.monitor_init().await.unwrap();

    let (handle, strong) = child.into_parts();
    strong.signal_shutdown();
    let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
    assert!(result.unwrap().is_ok());
}

// ============================================================================
// IntoDyn / AsDyn: converting between static and dynamic contexts.
// ============================================================================

#[tokio::test]
async fn into_dyn_widens_to_an_accepted_subset() {
    let child = spawn_rand(ping_pong_handler);
    child.monitor_init().await.unwrap();

    let address = child.address().clone();
    let dyn_address = address.into_dyn::<(Ping,)>();

    assert!(dyn_address.accepts::<Ping>());
    dyn_address.cast(Ping).await.unwrap();

    child.signal_shutdown();
}

#[tokio::test]
async fn into_dyn_checked_fails_for_a_message_the_interface_does_not_accept() {
    let child = spawn_rand(ping_pong_handler);
    child.monitor_init().await.unwrap();

    let address = child.address().clone();
    let result = address.into_dyn_checked::<(u8,)>();
    assert!(result.is_err(), "PingPongInterface does not accept u8");

    let address = result.unwrap_err();
    let result = address.into_dyn_checked::<(Ping, Pong)>();
    assert!(result.is_ok());

    child.signal_shutdown();
}

#[tokio::test]
async fn downcast_round_trips_to_the_concrete_interface() {
    let child = spawn_rand(ping_pong_handler);
    child.monitor_init().await.unwrap();

    let dyn_address = child.address().clone().into_dyn::<(Ping, Pong)>();

    let wrong = dyn_address.downcast::<UnrelatedInterface>();
    assert!(
        wrong.is_err(),
        "the channel's concrete interface is PingPongInterface, not UnrelatedInterface"
    );

    let dyn_address = wrong.unwrap_err();
    let concrete = dyn_address
        .downcast::<PingPongInterface>()
        .expect("should downcast back to the concrete interface");
    concrete.cast(Ping).await.unwrap();

    child.signal_shutdown();
}

#[tokio::test]
async fn as_dyn_and_downcast_ref_convert_by_reference() {
    let child = spawn_rand(ping_pong_handler);
    child.monitor_init().await.unwrap();

    let address = child.address();
    let as_dyn: &Address<Dyn<(Ping,)>> = address.as_dyn::<(Ping,)>();
    assert!(as_dyn.accepts::<Ping>());

    assert!(address.as_dyn_checked::<(u8,)>().is_none());
    assert!(address.as_dyn_checked::<(Ping,)>().is_some());

    assert!(address.downcast_ref::<UnrelatedInterface>().is_none());
    assert!(address.downcast_ref::<PingPongInterface>().is_some());

    child.signal_shutdown();
}
