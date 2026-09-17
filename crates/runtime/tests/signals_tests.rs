//! Tests for `Signal`/`SignalInterface` (src/signals.rs) and the signal-related
//! parts of `ActorOps` (src/ops.rs): `signal_shutdown`/`signal_suspend`/
//! `signal_resume`/`ping`, and the priority signals get over queued messages.

use zestors_interface::{Envelope, Interface, Message};
use zestors_runtime::prelude::*;
use zestors_runtime::{Signal, spawn_rand};

mod common;

#[derive(Message, Debug)]
#[msg(reply = ())]
#[msg(path = "zestors_interface")]
struct Ack;

#[derive(Interface, Debug)]
#[interface(path = "zestors_interface")]
enum AckInterface {
    Ack(Envelope<Ack>),
}

// ============================================================================
// Shutdown.
// ============================================================================

#[tokio::test]
async fn shutdown_stops_the_actor() {
    let child = spawn_rand(common::simplest_handler);
    child.watch_init().await.unwrap();

    assert!(child.signal_shutdown());
    child.watch_exit().await.unwrap();
    assert!(child.is_dead());
}

#[tokio::test]
async fn shutdown_returns_false_once_already_dead() {
    let child = spawn_rand(common::simplest_handler);
    child.signal_shutdown();
    child.watch_exit().await.unwrap();

    assert!(!child.signal_shutdown());
}

#[tokio::test]
async fn a_signal_queued_right_after_shutdown_on_an_idle_actor_outlives_the_event_loop() {
    // On an actor with an empty message queue, popping `Shutdown` makes the
    // very next check in `Channel::next_event` short-circuit straight to
    // `None` (its `Exiting if msg_is_empty() => None` guard doesn't look at
    // the signal queue again) - so a signal queued right behind `Shutdown`,
    // like this `ping`, is never seen by the running loop. It's still
    // answered - but only by `Inbox::drop`'s `drain_messages_and_signals`,
    // by which point the actor has already fully exited.
    let child = spawn_rand(common::simplest_handler);
    child.watch_init().await.unwrap();

    child.signal_shutdown();
    child.ping().await.unwrap();

    assert!(child.is_dead(), "the ping only resolved because the actor was already gone");
}

#[tokio::test]
async fn shutdown_still_drains_every_queued_message() {
    // The actor increments a shared counter for every message it processes;
    // shutting down must not cut that off early. Casting `Bump` 50 times and
    // then shutting down, then observing the *final* counter value once the
    // actor has fully exited, proves every message was handled - no sleeps,
    // no polling, just waiting for the terminal state.
    let counter = common::MessageCounter::new();
    let c = counter.clone();

    let child = spawn_rand(move |mut inbox: Inbox<()>| async move {
        while inbox.recv().await.is_some() {
            c.increment();
        }
        Ok::<_, rootcause::Report>(())
    });

    child.watch_init().await.unwrap();

    for _ in 0..50 {
        child.cast(()).await.unwrap();
    }
    child.signal_shutdown();

    child.watch_exit().await.unwrap();
    assert_eq!(counter.get(), 50);
}

// ============================================================================
// Suspend / resume.
// ============================================================================

#[tokio::test]
async fn suspend_then_resume_round_trips_status() {
    let child = spawn_rand(common::simplest_handler);
    child.watch_init().await.unwrap();

    assert!(child.signal_suspend());
    assert!(common::wait_for_suspended(&child).await);

    assert!(child.signal_resume());
    assert!(common::wait_for_running(&child).await);

    child.signal_shutdown();
}

#[tokio::test]
async fn suspend_is_a_no_op_when_already_suspended() {
    let child = spawn_rand(common::simplest_handler);
    child.signal_suspend();
    common::wait_for_suspended(&child).await;

    // The signal is still accepted (it doesn't fail), it just doesn't change
    // anything.
    assert!(child.signal_suspend());
    assert!(child.status().is_suspended());

    child.signal_shutdown();
}

#[tokio::test]
async fn resume_is_a_no_op_when_already_running() {
    let child = spawn_rand(common::simplest_handler);
    child.watch_init().await.unwrap();

    assert!(child.signal_resume());
    assert!(child.status().is_running());

    child.signal_shutdown();
}

#[tokio::test]
async fn signals_are_rejected_once_the_actor_is_exiting_but_not_yet_dead() {
    // Neither transition here needs an event: `register_initialized` and
    // `register_exiting` are plain synchronous calls, so by the time the
    // task has been polled once, it's already parked in `pending()` with
    // status `Exiting` - no signal or message was ever involved.
    let child = spawn_rand(|mut inbox: Inbox<()>| async move {
        inbox.register_initialized();
        inbox.register_exiting();
        std::future::pending::<()>().await;
        Ok(())
    });

    assert!(common::wait_for_exiting(&child).await);

    // `Channel::signal` refuses to even enqueue a new signal once the
    // channel is `Exiting`, regardless of which one it is.
    assert!(!child.signal_suspend());
    assert!(!child.signal_resume());
    assert!(!child.signal_shutdown());
    assert!(child.is_exiting(), "rejected signals must not change the status");

    child.abort();
}

#[tokio::test]
async fn suspend_prevents_processing_until_resumed() {
    // `Ack` is a request/reply message, so `call` lets us wait (with a
    // bound) for "was this actually processed", rather than "did some time
    // pass" - the timeout below only proves absence within a window many
    // times larger than any real processing would take.
    let child = spawn_rand(|mut inbox: Inbox<AckInterface>| async move {
        while let Some(AckInterface::Ack(envelope)) = inbox.recv().await {
            let _ = envelope.reply(());
        }
        Ok::<_, rootcause::Report>(())
    });
    child.watch_init().await.unwrap();

    child.signal_suspend();
    assert!(common::wait_for_suspended(&child).await);

    let stayed_suspended = tokio::time::timeout(std::time::Duration::from_millis(200), child.call(Ack))
        .await
        .is_err();
    assert!(stayed_suspended, "a suspended actor must not answer a call");

    child.signal_resume();
    assert!(common::wait_for_running(&child).await);

    let resumed_promptly = tokio::time::timeout(std::time::Duration::from_secs(2), child.call(Ack))
        .await;
    assert!(resumed_promptly.is_ok(), "a resumed actor must process its backlog");

    child.signal_shutdown();
}

#[tokio::test]
async fn messages_queued_while_suspended_are_processed_exactly_once_after_resuming() {
    let counter = common::MessageCounter::new();
    let c = counter.clone();

    let child = spawn_rand(move |mut inbox: Inbox<()>| async move {
        while inbox.recv().await.is_some() {
            c.increment();
        }
        Ok::<_, rootcause::Report>(())
    });
    child.watch_init().await.unwrap();

    child.signal_suspend();
    assert!(common::wait_for_suspended(&child).await);

    for _ in 0..5 {
        child.cast(()).await.unwrap();
    }

    child.signal_resume();
    assert!(common::wait_for_running(&child).await);

    child.signal_shutdown();
    child.watch_exit().await.unwrap();
    assert_eq!(counter.get(), 5);
}

// ============================================================================
// Ping.
// ============================================================================

#[tokio::test]
async fn ping_resolves_on_a_live_actor() {
    let child = spawn_rand(common::simplest_handler);
    child.watch_init().await.unwrap();

    assert!(child.ping().await.is_ok());

    child.signal_shutdown();
}

#[tokio::test]
async fn ping_fails_once_the_actor_is_dead() {
    let child = spawn_rand(common::simplest_handler);
    child.signal_shutdown();
    child.watch_exit().await.unwrap();

    assert!(child.ping().await.is_err());
}

#[tokio::test]
async fn ping_jumps_ahead_of_a_slow_message_backlog() {
    // `next_event`'s `select!` is biased toward signals, so a `ping` queued
    // behind a pile of slow-to-process messages must still resolve quickly:
    // the actor checks for a signal before popping its next message on
    // every single iteration of its loop, not just when it starts idle.
    let child = spawn_rand(|mut inbox: Inbox<()>| async move {
        while let Some(evt) = inbox.recv_event().await {
            if let InboxEvent::Message(_) = evt {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
        Ok::<_, rootcause::Report>(())
    });
    child.watch_init().await.unwrap();

    // 20 messages at 50ms/message would take a full second to drain.
    for _ in 0..20 {
        child.cast(()).await.unwrap();
    }

    let elapsed = {
        let start = std::time::Instant::now();
        child.ping().await.unwrap();
        start.elapsed()
    };
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "ping took {elapsed:?}, which suggests it waited behind the message backlog instead of cutting ahead of it"
    );

    child.signal_shutdown();
}

// ============================================================================
// Signal ordering and dead-channel behavior.
// ============================================================================

#[tokio::test]
async fn later_signals_override_earlier_ones_in_order() {
    let child = spawn_rand(common::simplest_handler);
    child.watch_init().await.unwrap();

    // Enqueue suspend, resume, suspend, back to back before the actor has a
    // chance to process any of them.
    child.signal_suspend();
    child.signal_resume();
    child.signal_suspend();

    assert!(common::wait_for_suspended(&child).await);
    assert!(child.status().is_suspended());

    child.signal_shutdown();
}

#[tokio::test]
async fn every_signal_returns_false_on_a_dead_channel() {
    let child = spawn_rand(common::simplest_handler);
    child.signal_shutdown();
    child.watch_exit().await.unwrap();

    assert!(!child.signal_suspend());
    assert!(!child.signal_resume());
    assert!(!child.signal(Signal::Shutdown));
}

#[tokio::test]
async fn signal_predicates_match_the_variant() {
    assert!(Signal::Shutdown.is_shutdown());
    assert!(!Signal::Shutdown.is_resume());
    assert!(!Signal::Shutdown.is_suspend());

    assert!(Signal::Suspend.is_suspend());
    assert!(!Signal::Suspend.is_shutdown());

    assert!(Signal::Resume.is_resume());
    assert!(!Signal::Resume.is_shutdown());
}
