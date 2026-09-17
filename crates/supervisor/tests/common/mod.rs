//! Shared setup for the supervision strategy tests: a minimal, controllable
//! actor plus small async-polling helpers, so the tests themselves only have
//! to describe "who crashes" and "who should/shouldn't restart."

use rootcause::{Report, report};
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use zestors_actor::{Actor, RestartMode, fn_blueprint};
use zestors_interface::{Envelope, Interface, Message};
use zestors_runtime::{Dyn, prelude::*};
use zestors_supervision::{BlueprintSupervisionExt as _, ChildConfig, ChildSpec, RestartIntensity};
use zestors_supervisor::{SupervisorBlueprint, SupervisorInterface};

/// Tells a [`TestActor`] to exit with an error, simulating an unhandled crash.
#[derive(Message, Debug)]
#[msg(path = "zestors_interface", reply = "()")]
pub struct Crash;

/// Asks a [`TestActor`] which "generation" it currently is: how many times
/// (including this one) it has been spawned.
#[derive(Message, Debug)]
#[msg(path = "zestors_interface", reply = "usize")]
pub struct Generation;

#[derive(Interface, Debug)]
#[interface(path = "zestors_interface")]
pub enum TestInterface {
    Crash(Envelope<Crash>),
    Generation(Envelope<Generation>),
}

/// A minimal actor for exercising supervision strategies. It does nothing on
/// its own: it bumps a shared counter every time it (re)starts, answers
/// [`Generation`] queries with that counter, and exits with an error when
/// sent [`Crash`] (simulating an unhandled failure while running).
pub struct TestActor {
    generation: Arc<AtomicUsize>,
}

impl Actor for TestActor {
    type Interface = TestInterface;
    type Exit = ();

    async fn run(self, mut inbox: Inbox<Self::Interface>) -> Result<(), Report> {
        self.generation.fetch_add(1, Ordering::SeqCst);

        loop {
            match inbox.recv_event().await {
                Some(InboxEvent::Message(TestInterface::Crash(_))) => {
                    return Err(report!("simulated crash"));
                }
                Some(InboxEvent::Message(TestInterface::Generation(Envelope {
                    req: request,
                    ..
                }))) => {
                    request.reply(self.generation.load(Ordering::SeqCst)).ok();
                }
                Some(InboxEvent::Signal(signal)) if signal.is_shutdown() => return Ok(()),
                Some(_) => {}
                None => return Ok(()),
            }
        }
    }
}

/// An actor that takes a configurable amount of time to actually exit once
/// asked to shut down, so tests can observe a supervisor's state while one
/// of its supervisees is still alive but mid-shutdown.
pub struct SlowShutdownActor {
    delay: Duration,
}

impl Actor for SlowShutdownActor {
    type Interface = TestInterface;
    type Exit = ();

    async fn run(self, mut inbox: Inbox<Self::Interface>) -> Result<(), Report> {
        loop {
            match inbox.recv_event().await {
                Some(InboxEvent::Signal(signal)) if signal.is_shutdown() => {
                    tokio::time::sleep(self.delay).await;
                    return Ok(());
                }
                Some(_) => {}
                None => return Ok(()),
            }
        }
    }
}

/// Builds a [`SlowShutdownActor`] child (random pid) that sleeps for `delay`
/// before actually exiting once told to stop.
pub fn slow_shutdown_child(delay: Duration) -> ChildSpec {
    fn_blueprint(move || SlowShutdownActor { delay })
        .with_rand_pid()
        .split()
        .0
}

/// Builds a fresh [`TestActor`] child (random pid, short timeouts so a
/// misbehaving test fails fast instead of hanging), returning its spec (to
/// hand to a [`SupervisorBlueprint`]), its address (to send it [`Crash`] /
/// [`Generation`] directly, bypassing the supervisor), and its generation
/// counter.
pub fn test_child(mode: RestartMode) -> (ChildSpec, Address<TestInterface>, Arc<AtomicUsize>) {
    let generation = Arc::new(AtomicUsize::new(0));
    let for_actor = generation.clone();

    let (spec, address) = fn_blueprint(move || TestActor {
        generation: for_actor.clone(),
    })
    .with_rand_pid()
    .with_cfg(ChildConfig {
        restart_mode: mode,
        // The channel a just-stopped child was on can briefly still report
        // itself as not-yet-dead, so restarting it can take a couple of
        // quick, self-healing retries (surfaced as an ordinary start
        // failure). A generous intensity keeps that from tripping the
        // restart limiter and shutting the test's supervisor down early.
        intensity: Some(RestartIntensity::restarts(100).within(Duration::from_secs(30))),
        abort_timeout: Duration::from_secs(1),
        init_timeout: Duration::from_secs(1),
        start_timeout: Duration::from_secs(1),
    })
    .split();

    (spec, address, generation)
}

/// Starts a [`SupervisorBlueprint`] directly (no [`Node`](zestors_supervisor::Node) —
/// its exit path calls `std::process::exit`, which would kill the test
/// process), returning the running supervisor's own [`Child`] handle (keep
/// this alive for the test's duration; dropping it aborts the whole tree)
/// and its address.
pub async fn spawn_supervisor(
    blueprint: SupervisorBlueprint,
) -> (Child<(), Dyn>, Address<SupervisorInterface>) {
    let (spec, address) = blueprint.with_rand_pid().split();
    let child = spec.start().await.expect("supervisor should start");
    (child, address)
}

/// Polls `check` until it resolves `true`, or panics once `timeout` elapses.
/// Used throughout instead of a fixed sleep, since restarts happen
/// asynchronously and shouldn't be raced.
pub async fn wait_for<Fut>(timeout: Duration, mut check: impl FnMut() -> Fut)
where
    Fut: Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if check().await {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "condition not met within {timeout:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Waits until `address` reports the given generation. Retries on transient
/// send/timeout errors too, since there's a brief window between the old
/// instance exiting and the new one starting where the channel may not yet
/// accept messages.
pub async fn wait_for_generation(
    address: &Address<TestInterface>,
    expected: usize,
    timeout: Duration,
) {
    wait_for(timeout, || async {
        matches!(
            tokio::time::timeout(Duration::from_millis(200), address.call_dyn(Generation)).await,
            Ok(Ok(g)) if g == expected
        )
    })
    .await;
}
