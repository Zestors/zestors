//! Membership scenarios on a simulated network and virtual time.
//!
//! These run in milliseconds of real time however long the scenario is, and
//! can cut the network, which the QUIC tests in `cluster.rs` can't.

use std::{future::Future, net::SocketAddr, num::NonZeroU32, time::Duration};
use tokio::sync::broadcast;
use zestors_distr::{
    ClusterEvent, NodeName,
    sim::{SimNetwork, SimNode},
};

fn addr(n: u8) -> SocketAddr {
    SocketAddr::from(([10, 0, 0, n], 7000))
}

/// Failure detection tuned so that scenarios stay short.
fn fast_foca_config() -> foca::Config {
    let mut config = foca::Config::new_lan(NonZeroU32::new(3).unwrap());
    config.probe_period = Duration::from_millis(100);
    config.probe_rtt = Duration::from_millis(50);
    config.suspect_to_down_after = Duration::from_millis(200);
    if let Some(gossip) = &mut config.periodic_gossip {
        gossip.frequency = Duration::from_millis(50);
    }
    if let Some(announce) = &mut config.periodic_announce_to_down_members {
        announce.frequency = Duration::from_secs(1);
    }
    config
}

fn new_network(seed: u64) -> SimNetwork {
    let mut sim = SimNetwork::new(seed);
    *sim.foca_config_mut() = fast_foca_config();
    sim
}

/// Fails the test if `future` takes unreasonably long in virtual time.
async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(120), future)
        .await
        .expect("Timed out")
}

async fn members_of(node: &SimNode, count: usize) {
    within(node.cluster.wait_for_members(count)).await;
}

/// The first event that says `node` is gone.
async fn departure_of(rx: &mut broadcast::Receiver<ClusterEvent>, node: &str) -> ClusterEvent {
    within(async {
        loop {
            match rx.recv().await {
                Ok(event)
                    if matches!(event, ClusterEvent::Left(_) | ClusterEvent::Failed(_))
                        && event.member().name.as_str() == node =>
                {
                    return event;
                }
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => panic!("Event stream closed"),
            }
        }
    })
    .await
}

/// Shuts a node down through its root supervisor, once that takes signals: it
/// is initializing or running.
async fn stop(root: &zestors::runtime::Address<zestors_supervisor::SupervisorInterface>) {
    use zestors::runtime::prelude::*;
    root.monitor_accepts_messages().await;
    root.signal_shutdown();
}

#[tokio::test(start_paused = true)]
async fn nodes_discover_each_other_and_notice_departures() {
    let mut sim = new_network(1);
    let mut a = sim.start("node-a", addr(1), &[]).await;
    let mut b = sim.start("node-b", addr(2), &[("node-a", addr(1))]).await;
    let mut c = sim.start("node-c", addr(3), &[("node-a", addr(1))]).await;
    for node in [&a, &b, &c] {
        members_of(node, 2).await;
    }

    let mut a_events = a.cluster.subscribe();

    // A node that leaves is reported as having left.
    c.leave().await;
    assert!(matches!(
        departure_of(&mut a_events, "node-c").await,
        ClusterEvent::Left(_)
    ));
    members_of(&a, 1).await;
    members_of(&b, 1).await;

    // A node is identified by its name, not its address: restarted somewhere
    // else, it replaces its previous incarnation.
    let old_generation = c.cluster.local_member().generation;
    let c2 = sim.start("node-c", addr(4), &[("node-a", addr(1))]).await;
    members_of(&a, 2).await;
    members_of(&c2, 2).await;
    assert_eq!(
        a.cluster.member(&NodeName::new("node-c")).map(|m| m.addr),
        Some(addr(4).into())
    );
    assert!(c2.cluster.local_member().generation > old_generation);

    // A node that vanishes without saying goodbye is reported as failed.
    b.crash().await;
    assert!(matches!(
        departure_of(&mut a_events, "node-b").await,
        ClusterEvent::Failed(_)
    ));
    members_of(&a, 1).await;
    members_of(&c2, 1).await;

    a.leave().await;
}

/// A node that stops answering is reported unreachable, while still a member,
/// long before the (here deliberately slow) failure detector declares it down.
#[tokio::test(start_paused = true)]
async fn crashed_node_is_unreachable_before_it_is_failed() {
    let mut sim = new_network(2);
    sim.foca_config_mut().suspect_to_down_after = Duration::from_secs(4);
    let a = sim.start("node-a", addr(1), &[]).await;
    let mut b = sim.start("node-b", addr(2), &[("node-a", addr(1))]).await;
    let b_id = NodeName::new("node-b");
    members_of(&a, 1).await;

    let mut events = a.cluster.subscribe();
    b.crash().await;

    let mut saw_unreachable = false;
    loop {
        match within(events.recv()).await.expect("Event stream open") {
            ClusterEvent::Unreachable(m) => {
                assert_eq!(m.name, b_id);
                assert!(a.cluster.member(&b_id).is_some(), "still a member");
                assert!(!a.cluster.is_reachable(&b_id));
                saw_unreachable = true;
            }
            ClusterEvent::Failed(m) => {
                assert_eq!(m.name, b_id);
                break;
            }
            _ => {}
        }
    }
    assert!(saw_unreachable, "reported unreachable before failed");
    assert!(a.cluster.member(&b_id).is_none());
    assert!(!a.cluster.is_reachable(&b_id));
}

#[tokio::test(start_paused = true)]
async fn snapshot_and_events_line_up() {
    let mut sim = new_network(3);
    let a = sim.start("node-a", addr(1), &[]).await;

    let (snapshot, mut events) = a.cluster.subscribe_with_snapshot();
    assert!(snapshot.members.is_empty());

    let _b = sim.start("node-b", addr(2), &[("node-a", addr(1))]).await;
    let b_id = NodeName::new("node-b");

    // The change made after the snapshot arrives as an event...
    let up = within(async {
        loop {
            if let Ok(event @ ClusterEvent::Up(_)) = events.recv().await {
                break event;
            }
        }
    })
    .await;
    assert_eq!(up.member().name, b_id);

    // ...and a later snapshot contains it, with no second event for it.
    let (snapshot, mut events) = a.cluster.subscribe_with_snapshot();
    assert_eq!(snapshot.members.len(), 1);
    assert!(snapshot.is_reachable(&b_id));
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(matches!(
        events.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // wait_until sees the state at once when it already holds.
    let members = within(a.cluster.wait_until(|m| m.iter().any(|m| m.name == b_id))).await;
    assert_eq!(members.len(), 1);
}

/// A cluster that is split in two and then reconnected becomes one again,
/// without anyone restarting.
#[tokio::test(start_paused = true)]
async fn partitioned_cluster_heals() {
    let mut sim = new_network(4);
    let a = sim.start("node-a", addr(1), &[]).await;
    let b = sim.start("node-b", addr(2), &[("node-a", addr(1))]).await;
    let c = sim.start("node-c", addr(3), &[("node-a", addr(1))]).await;
    for node in [&a, &b, &c] {
        members_of(node, 2).await;
    }

    sim.partition(&["node-a"], &["node-b", "node-c"]);
    members_of(&a, 0).await;
    members_of(&b, 1).await;
    members_of(&c, 1).await;

    sim.heal();
    for node in [&a, &b, &c] {
        members_of(node, 2).await;
    }
}

/// With the same seed, two runs of a scenario see the same events in the same order.
#[tokio::test(start_paused = true)]
async fn runs_are_reproducible() {
    async fn run(seed: u64) -> Vec<ClusterEvent> {
        let mut sim = new_network(seed);
        sim.set_latency(Duration::from_millis(5), Duration::from_millis(20));
        let a = sim.start("node-a", addr(1), &[]).await;
        let mut events = a.cluster.subscribe();
        let _b = sim.start("node-b", addr(2), &[("node-a", addr(1))]).await;
        let mut c = sim.start("node-c", addr(3), &[("node-a", addr(1))]).await;
        let _d = sim.start("node-d", addr(4), &[("node-a", addr(1))]).await;
        members_of(&a, 3).await;
        c.crash().await;
        members_of(&a, 2).await;

        let mut seen = Vec::new();
        while let Ok(event) = events.try_recv() {
            seen.push(event);
        }
        seen
    }

    let first = run(7).await;
    assert!(first.len() >= 4, "the scenario produced events: {first:?}");
    assert_eq!(first, run(7).await);
}

/// Real cluster nodes, supervisor and all, over the simulated network.
#[tokio::test(start_paused = true)]
async fn cluster_nodes_run_over_the_simulated_network() {
    use zestors::{prelude::*, supervisor::Supervisor};
    use zestors_distr::{ClusterConfig, ClusterNode, Seed};

    let net = SimNetwork::new(5);
    let config = |name: &str, n: u8| {
        ClusterConfig::new(name, net.backend(addr(n))).foca_config(fast_foca_config())
    };
    let a = ClusterNode::new(Supervisor::blueprint().rand_name(), config("node-a", 1));
    let b = ClusterNode::new(
        Supervisor::blueprint().rand_name(),
        config("node-b", 2).seed(Seed::new("node-a", addr(1))),
    );

    let (a_cluster, b_cluster) = (a.cluster(), b.cluster());
    let (a_shutdown, b_shutdown) = (
        a.root_supervisor().address().clone(),
        b.root_supervisor().address().clone(),
    );
    let (a_task, b_task) = (tokio::spawn(a.run()), tokio::spawn(b.run()));

    within(a_cluster.wait_for_members(1)).await;
    within(b_cluster.wait_for_members(1)).await;

    stop(&b_shutdown).await;
    b_task.await.unwrap().unwrap();
    within(a_cluster.wait_for_members(0)).await;

    stop(&a_shutdown).await;
    a_task.await.unwrap().unwrap();
}
