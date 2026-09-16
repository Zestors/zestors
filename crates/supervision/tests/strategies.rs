//! Regression tests for the three restart strategies. Each test only checks
//! the one behavior that actually distinguishes that strategy (or, for the
//! last test, a specific bug this crate had earlier): who restarts and who
//! doesn't when a single child crashes.

mod common;

use common::{Crash, spawn_supervisor, test_child, wait_for, wait_for_generation};
use std::{sync::atomic::Ordering, time::Duration};
use zestors_actor::RestartMode;
use zestors_runtime::prelude::*;
use zestors_supervision::{GetChildren, SupervisorBlueprint};

const TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn one_for_one_only_restarts_the_crashed_child() {
    let (spec_a, addr_a, gen_a) = test_child(RestartMode::OnError);
    let (spec_b, addr_b, _gen_b) = test_child(RestartMode::OnError);
    let (spec_c, addr_c, gen_c) = test_child(RestartMode::OnError);

    let (_supervisor, _addr) =
        spawn_supervisor(SupervisorBlueprint::one_for_one().children([spec_a, spec_b, spec_c]))
            .await;

    wait_for_generation(&addr_a, 1, TIMEOUT).await;
    wait_for_generation(&addr_b, 1, TIMEOUT).await;
    wait_for_generation(&addr_c, 1, TIMEOUT).await;

    addr_b.call_dyn(Crash).await.ok();
    wait_for_generation(&addr_b, 2, TIMEOUT).await;

    assert_eq!(
        gen_a.load(Ordering::SeqCst),
        1,
        "A should not have been touched"
    );
    assert_eq!(
        gen_c.load(Ordering::SeqCst),
        1,
        "C should not have been touched"
    );
}

#[tokio::test]
async fn one_for_all_restarts_every_sibling_and_drops_never_mode() {
    let (spec_a, addr_a, _gen_a) = test_child(RestartMode::OnError);
    let (spec_b, addr_b, _gen_b) = test_child(RestartMode::OnError);
    let (spec_c, addr_c, _gen_c) = test_child(RestartMode::OnError);
    let (spec_d, addr_d, _gen_d) = test_child(RestartMode::Never);

    let (_supervisor, supervisor_addr) = spawn_supervisor(
        SupervisorBlueprint::one_for_all().children([spec_a, spec_b, spec_c, spec_d]),
    )
    .await;

    wait_for_generation(&addr_a, 1, TIMEOUT).await;
    wait_for_generation(&addr_b, 1, TIMEOUT).await;
    wait_for_generation(&addr_c, 1, TIMEOUT).await;
    wait_for_generation(&addr_d, 1, TIMEOUT).await;

    addr_b.call_dyn(Crash).await.ok();

    // The whole group restarts together...
    wait_for_generation(&addr_a, 2, TIMEOUT).await;
    wait_for_generation(&addr_b, 2, TIMEOUT).await;
    wait_for_generation(&addr_c, 2, TIMEOUT).await;

    // ...except D, which was configured `RestartMode::Never`: it should be
    // dropped from the group entirely rather than brought back.
    wait_for(TIMEOUT, || async { addr_d.is_dead() }).await;
    wait_for(TIMEOUT, || async {
        matches!(
            tokio::time::timeout(Duration::from_millis(200), supervisor_addr.call_dyn(GetChildren)).await,
            Ok(Ok(children)) if children.len() == 3
        )
    })
    .await;
}

#[tokio::test]
async fn rest_for_one_restarts_the_crashed_child_and_everything_after_it() {
    let (spec_a, addr_a, gen_a) = test_child(RestartMode::OnError);
    let (spec_b, addr_b, _gen_b) = test_child(RestartMode::OnError);
    let (spec_c, addr_c, _gen_c) = test_child(RestartMode::OnError);
    let (spec_d, addr_d, _gen_d) = test_child(RestartMode::OnError);

    // Start order matters here: A, then B, then C, then D.
    let (_supervisor, _addr) = spawn_supervisor(
        SupervisorBlueprint::rest_for_one().children([spec_a, spec_b, spec_c, spec_d]),
    )
    .await;

    wait_for_generation(&addr_a, 1, TIMEOUT).await;
    wait_for_generation(&addr_b, 1, TIMEOUT).await;
    wait_for_generation(&addr_c, 1, TIMEOUT).await;
    wait_for_generation(&addr_d, 1, TIMEOUT).await;

    addr_b.call_dyn(Crash).await.ok();

    // B and everything started after it (C, D) come back...
    wait_for_generation(&addr_b, 2, TIMEOUT).await;
    wait_for_generation(&addr_c, 2, TIMEOUT).await;
    wait_for_generation(&addr_d, 2, TIMEOUT).await;

    // ...but A, started before it, is left alone.
    assert_eq!(
        gen_a.load(Ordering::SeqCst),
        1,
        "A should not have been touched"
    );
}

/// Regression test: a supervisor shutdown must not let an `Always`-mode
/// child's own exit (which, under a graceful shutdown, is a `NormalShutdown`
/// that `Always` would otherwise restart on) resurrect it mid-teardown.
#[tokio::test]
async fn shutdown_does_not_resurrect_always_mode_children() {
    let (spec_a, addr_a, gen_a) = test_child(RestartMode::Always);

    let (supervisor, _addr) =
        spawn_supervisor(SupervisorBlueprint::one_for_one().children([spec_a])).await;

    wait_for_generation(&addr_a, 1, TIMEOUT).await;

    supervisor.signal_shutdown();

    let result = tokio::time::timeout(TIMEOUT, supervisor).await;
    assert!(
        result.is_ok(),
        "supervisor did not exit within {TIMEOUT:?} of a shutdown signal"
    );

    assert_eq!(
        gen_a.load(Ordering::SeqCst),
        1,
        "an Always-mode child must not be restarted while its supervisor is shutting down"
    );
}
