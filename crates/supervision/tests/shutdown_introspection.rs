//! Regression tests for introspecting a supervisor (via [`GetChildren`])
//! while it is gracefully shutting down, rather than only before or after.

mod common;

use common::{slow_shutdown_child, spawn_supervisor, test_child, wait_for};
use std::time::Duration;
use zestors_actor::RestartMode;
use zestors_runtime::{CastOptions, prelude::*};
use zestors_supervision::{GetChildren, SupervisorBlueprint};

const TIMEOUT: Duration = Duration::from_secs(5);

/// A supervisor that has been told to shut down, but whose supervisees
/// haven't all exited yet, should still answer [`GetChildren`] with its full
/// list of supervisees — not an empty one. Otherwise, anything building a
/// tree from these responses (e.g. the inspector) sees a still-alive
/// supervisee's parent report zero children and orphans it in the rendered
/// tree, even though the supervisee is still up and the parent hasn't
/// exited either.
#[tokio::test]
async fn get_children_reports_supervisees_while_shutting_down() {
    let slow_spec = slow_shutdown_child(Duration::from_millis(300));
    let (fast_spec, _addr, _gen) = test_child(RestartMode::Never);

    let (_supervisor, supervisor_addr) =
        spawn_supervisor(SupervisorBlueprint::one_for_one().children([slow_spec, fast_spec])).await;

    // Wait until the supervisor has both supervisees up before shutting down.
    wait_for(TIMEOUT, || async {
        matches!(
            tokio::time::timeout(Duration::from_millis(200), supervisor_addr.call_dyn(GetChildren)).await,
            Ok(Ok(children)) if children.len() == 2
        )
    })
    .await;

    supervisor_addr.signal_shutdown();

    // Give the shutdown signal time to be processed (the supervisor should
    // now be `Exiting`), but stay well inside the slow child's 300ms exit
    // delay so at least one supervisee is still alive.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let children = tokio::time::timeout(
        Duration::from_millis(200),
        supervisor_addr.call_dyn_with(GetChildren, CastOptions::default().ignore_exiting(true)),
    )
    .await
    .expect("GetChildren should not time out while the supervisor is stopping")
    .expect("supervisor should still answer GetChildren while stopping");

    assert_eq!(
        children.len(),
        2,
        "supervisor should still report both supervisees while gracefully shutting down, got {children:?}"
    );
}
