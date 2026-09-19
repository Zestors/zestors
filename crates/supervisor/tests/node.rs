//! Tests for [`Node`] lifecycle.

use std::time::Duration;
use zestors_supervision::BlueprintSupervisionExt as _;
use zestors_supervisor::{Node, Supervisor};

const TIMEOUT: Duration = Duration::from_secs(5);

/// A shutdown requested as soon as [`Node::run`] is called — possibly before
/// the supervisor has started — must still shut the node down, not be lost.
#[tokio::test(flavor = "multi_thread")]
async fn shutdown_requested_immediately_is_not_lost() {
    for _ in 0..20 {
        let node = Node::new(Supervisor::blueprint().rand_name()).with_exit_delay(Duration::ZERO);
        let shutdown = node.shutdown_handle();

        let handle = tokio::spawn(node.run());
        shutdown.shutdown();

        tokio::time::timeout(TIMEOUT, handle)
            .await
            .expect("node exits after a single, early shutdown request")
            .unwrap()
            .unwrap();
    }
}

/// A shutdown requested before [`Node::run`] is even called is honored too.
#[tokio::test]
async fn shutdown_requested_before_run_is_honored() {
    let node = Node::new(Supervisor::blueprint().rand_name()).with_exit_delay(Duration::ZERO);
    node.shutdown_handle().shutdown();

    tokio::time::timeout(TIMEOUT, node.run())
        .await
        .expect("node exits")
        .unwrap();
}
