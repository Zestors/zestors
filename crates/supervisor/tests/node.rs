//! Tests for [`Node`] lifecycle.

use std::time::Duration;
use zestors_runtime::prelude::*;
use zestors_supervision::BlueprintSupervisionExt as _;
use zestors_supervisor::{Node, Supervisor};

const TIMEOUT: Duration = Duration::from_secs(5);

/// Once the root supervisor takes signals (it is initializing or running), a
/// shutdown signal to it stops the node.
#[tokio::test(flavor = "multi_thread")]
async fn shutdown_once_running_stops_the_node() {
    for _ in 0..20 {
        let node = Node::new(Supervisor::blueprint().rand_name());
        let root = node.root_supervisor().address().clone();

        let handle = tokio::spawn(node.run());
        tokio::time::timeout(TIMEOUT, root.monitor_accepts_messages())
            .await
            .expect("the root supervisor starts");
        assert!(root.signal_shutdown());

        tokio::time::timeout(TIMEOUT, handle)
            .await
            .expect("node exits after a single shutdown signal")
            .unwrap()
            .unwrap();
    }
}
