//! Tests for sending messages: `Accepts`/`ActorOps`'s `cast`/`try_cast`/
//! `call` family (src/accepts.rs, src/ops.rs) and their dynamic
//! (`*_dyn`) counterparts, plus the backpressure they respect
//! (src/queue/backpressure.rs).

use rootcause::Report;
use std::time::Duration;
use zestors_interface::{Envelope, Interface, Message};
use zestors_runtime::errors::{CallDynError, CallError, CastDynError, TryCastDynError, TryCastError};
use zestors_runtime::prelude::*;
use zestors_runtime::{CallOptions, spawn_rand};

mod common;

#[derive(Message, Debug, Clone)]
#[msg(path = "zestors_interface")]
struct Bump;

#[derive(Message, Debug)]
#[msg(reply = usize)]
#[msg(path = "zestors_interface")]
struct GetCount;

#[derive(Interface, Debug)]
#[interface(path = "zestors_interface")]
enum CounterInterface {
    Bump(Envelope<Bump>),
    GetCount(Envelope<GetCount>),
}

async fn counter_handler(mut inbox: Inbox<CounterInterface>) -> Result<(), Report> {
    let mut count = 0usize;
    while let Some(msg) = inbox.recv().await {
        match msg {
            CounterInterface::Bump(_) => count += 1,
            CounterInterface::GetCount(env) => {
                let _ = env.reply(count);
            }
        }
    }
    Ok(())
}

#[derive(Message, Debug)]
#[msg(reply = ())]
#[msg(path = "zestors_interface")]
struct NeverReplied;

#[derive(Interface, Debug)]
#[interface(path = "zestors_interface")]
enum NeverRepliesInterface {
    NeverReplied(Envelope<NeverReplied>),
}

// ============================================================================
// Regression test: `try_cast_with`'s static/typed path must wake a receiver
// that is already parked in `recv`. It didn't, until this suite caught it -
// see crates/runtime/src/channel/cast.rs.
// ============================================================================

#[tokio::test]
async fn cast_wakes_a_receiver_already_parked_in_recv() {
    let child = spawn_rand(counter_handler);
    // `watch_init` only resolves once the handler's first `recv` has run,
    // so by this point the actor is guaranteed to already be parked,
    // waiting on the (currently empty) queue.
    child.watch_init().await.unwrap();

    let delivered = tokio::time::timeout(Duration::from_secs(2), child.cast(Bump)).await;
    assert!(
        delivered.is_ok(),
        "cast must wake an actor that was already parked in recv()"
    );

    child.signal_shutdown();
}

// ============================================================================
// cast / try_cast.
// ============================================================================

#[tokio::test]
async fn casting_is_fifo_so_a_trailing_call_is_a_processing_barrier() {
    let child = spawn_rand(counter_handler);
    child.watch_init().await.unwrap();

    for _ in 0..200 {
        child.cast(Bump).await.unwrap();
    }
    // Every message goes through the same single per-actor queue, so this
    // can only resolve once all 200 prior `Bump`s have been popped and
    // handled, in order.
    let count = child.call(GetCount).await.unwrap();
    assert_eq!(count, 200);

    child.signal_shutdown();
}

#[tokio::test]
async fn try_cast_succeeds_on_a_running_actor() {
    let child = spawn_rand(counter_handler);
    child.watch_init().await.unwrap();

    assert!(child.try_cast(Bump).is_ok());

    child.signal_shutdown();
}

#[tokio::test]
async fn cast_and_try_cast_fail_once_the_actor_is_dead() {
    let child = spawn_rand(counter_handler);
    child.signal_shutdown();
    child.watch_exit().await.unwrap();

    assert!(matches!(child.cast(Bump).await, Err(_)));
    assert!(matches!(child.try_cast(Bump), Err(TryCastError::Closed(Bump))));
}

#[tokio::test]
async fn cast_fails_while_exiting_unless_told_to_ignore_it() {
    // A real actor that receives `Signal::Shutdown` with an empty message
    // queue exits immediately rather than lingering in `Exiting` (see
    // `Channel::next_event`), which would make that window racy to observe
    // from here. `register_exiting` reaches the exact same status
    // deterministically instead, without depending on that timing.
    let child = spawn_rand(|mut inbox: Inbox<CounterInterface>| async move {
        inbox.register_initialized();
        inbox.register_exiting();
        std::future::pending::<()>().await;
        Ok(())
    });
    assert!(common::wait_for_exiting(&child).await);

    // Default options respect `Exiting` as closed.
    let default_result = child.cast(Bump).await;
    assert!(default_result.is_err());

    // `ignore_exiting` lets it through while still `Exiting`.
    let options = CallOptions::new().ignore_exiting(true);
    let result = child.cast_with(Bump, options).await;
    assert!(result.is_ok());

    child.abort();
}

// ============================================================================
// call / call_with (request-reply).
// ============================================================================

#[tokio::test]
async fn call_returns_the_actors_reply() {
    let child = spawn_rand(counter_handler);
    child.watch_init().await.unwrap();

    child.cast(Bump).await.unwrap();
    child.cast(Bump).await.unwrap();
    assert_eq!(child.call(GetCount).await.unwrap(), 2);

    child.signal_shutdown();
}

#[tokio::test]
async fn call_fails_closed_on_a_dead_actor() {
    let child = spawn_rand(counter_handler);
    child.signal_shutdown();
    child.watch_exit().await.unwrap();

    assert!(matches!(child.call(GetCount).await, Err(CallError::Closed(GetCount))));
}

#[tokio::test]
async fn call_reports_no_response_if_the_request_is_dropped_unanswered() {
    let child = spawn_rand(|mut inbox: Inbox<NeverRepliesInterface>| async move {
        if let Some(NeverRepliesInterface::NeverReplied(envelope)) = inbox.recv().await {
            // Intentionally drop the envelope (and its `Request`) without
            // ever calling `reply`.
            drop(envelope);
        }
        Ok::<_, rootcause::Report>(())
    });
    child.watch_init().await.unwrap();

    let result = child.call(NeverReplied).await;
    assert!(matches!(result, Err(CallError::NoResponse)));
}

// ============================================================================
// Backpressure.
// ============================================================================

#[tokio::test]
async fn try_cast_reports_full_once_backpressure_is_reached() {
    // A handler that never calls `recv`, so nothing is ever drained and the
    // queue length only ever grows: no timing dependency at all.
    let child = spawn_rand(|_inbox: Inbox<CounterInterface>| async move {
        std::future::pending::<()>().await;
        Ok(())
    });

    let mut first_full_at = None;
    for i in 0.. {
        match child.try_cast(Bump) {
            Ok(_) => {}
            Err(TryCastError::Full(_)) => {
                first_full_at = Some(i);
                break;
            }
            Err(other) => panic!("unexpected error: {other:?}"),
        }
        if i > 200 {
            panic!("backpressure should have kicked in well before 200 messages");
        }
    }

    // The default config starts applying backpressure at 50% of the 100
    // message limit, i.e. somewhere around the 50th message.
    let first_full_at = first_full_at.unwrap();
    assert!(
        (30..=100).contains(&first_full_at),
        "expected backpressure around the 50-message mark, first Full at {first_full_at}"
    );

    child.abort();
}

#[tokio::test]
async fn ignore_backpressure_bypasses_the_full_error() {
    let child = spawn_rand(|_inbox: Inbox<CounterInterface>| async move {
        std::future::pending::<()>().await;
        Ok(())
    });

    let options = CallOptions::new().ignore_backpressure(true);
    for _ in 0..500 {
        child.try_cast_with(Bump, options).unwrap();
    }
    assert_eq!(child.msg_len(), 500);

    child.abort();
}

#[tokio::test]
async fn cast_waits_out_backpressure_instead_of_failing() {
    // Unlike `try_cast`, `cast` never fails just because the queue is
    // fuller than the backpressure threshold - it only errors if the
    // channel is actually closed.
    let child = spawn_rand(|_inbox: Inbox<CounterInterface>| async move {
        std::future::pending::<()>().await;
        Ok(())
    });

    for _ in 0..90 {
        child.try_cast(Bump).ok();
    }
    assert!(child.reached_backpressure());

    let result = tokio::time::timeout(Duration::from_secs(2), child.cast(Bump)).await;
    assert!(result.is_ok(), "cast() should have waited out backpressure rather than erroring");
    assert!(result.unwrap().is_ok());

    child.abort();
}

// ============================================================================
// Dynamic sends (`cast_dyn`/`try_cast_dyn`/`call_dyn`), and type introspection.
// ============================================================================

#[tokio::test]
async fn cast_dyn_and_call_dyn_work_for_an_accepted_message() {
    let child = spawn_rand(counter_handler);
    child.watch_init().await.unwrap();

    child.cast_dyn(Bump).await.unwrap();
    let count = child.call_dyn(GetCount).await.unwrap();
    assert_eq!(count, 1);

    child.signal_shutdown();
}

#[tokio::test]
async fn dyn_sends_reject_a_message_type_the_interface_does_not_accept() {
    let child = spawn_rand(counter_handler);
    child.watch_init().await.unwrap();

    // `u8` is a `Message` (see zestors-interface's blanket impls) but is not
    // one of `CounterInterface`'s variants.
    assert!(matches!(
        child.try_cast_dyn(7u8),
        Err(TryCastDynError::NotAccepted(7))
    ));
    assert!(matches!(
        child.cast_dyn(7u8).await,
        Err(CastDynError::NotAccepted(7))
    ));
    assert!(matches!(
        child.call_dyn(7u8).await,
        Err(CallDynError::NotAccepted(7))
    ));

    child.signal_shutdown();
}

#[tokio::test]
async fn members_accepts_and_is_interface_reflect_the_concrete_type() {
    let child = spawn_rand(counter_handler);
    child.watch_init().await.unwrap();

    assert!(child.accepts::<Bump>());
    assert!(child.accepts::<GetCount>());
    assert!(!child.accepts::<u8>());
    assert_eq!(child.members().len(), 2);

    assert!(child.is_interface::<CounterInterface>());
    assert!(!child.is_interface::<NeverRepliesInterface>());

    child.signal_shutdown();
}

// ============================================================================
// CallOptions.
// ============================================================================

#[test]
fn call_options_default_disables_both_flags() {
    let options = CallOptions::default();
    assert!(!options.ignore_exiting);
    assert!(!options.ignore_backpressure);
}

#[test]
fn call_options_builder_methods_set_the_right_flag() {
    let options = CallOptions::new().ignore_exiting(true);
    assert!(options.ignore_exiting);
    assert!(!options.ignore_backpressure);

    let options = CallOptions::new().ignore_backpressure(true);
    assert!(!options.ignore_exiting);
    assert!(options.ignore_backpressure);
}
