use std::{net::SocketAddr, num::NonZeroU32, time::Duration};
use tokio::{sync::broadcast, task::JoinHandle};
use zestors::{prelude::*, supervisor::Supervisor};
use zestors_distr::{
    Cluster, ClusterConfig, ClusterEvent, ClusterNode, ClusterNodeError, ClusterTimings,
    LinkTimings, NodeName, NodeStatus, Seed,
};
use zestors_distr_quic::{Quic, QuicTimings, Tls};

fn free_addr() -> SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

/// Failure detection tuned for localhost: a crash is noticed in well under a second.
fn fast_foca_config() -> foca::Config {
    let mut config = foca::Config::new_lan(NonZeroU32::new(3).unwrap());
    config.probe_period = Duration::from_millis(100);
    config.probe_rtt = Duration::from_millis(50);
    config.suspect_to_down_after = Duration::from_millis(200);
    if let Some(gossip) = &mut config.periodic_gossip {
        gossip.frequency = Duration::from_millis(50);
    }
    config
}

fn fast_link_timings() -> LinkTimings {
    LinkTimings {
        connect_timeout: Duration::from_millis(500),
        handshake_timeout: Duration::from_millis(500),
        peer_idle: Duration::from_secs(5),
        reconnect_backoff_min: Duration::from_millis(50),
        reconnect_backoff_max: Duration::from_millis(400),
        shutdown_grace: Duration::from_millis(200),
    }
}

fn fast_timings() -> ClusterTimings {
    ClusterTimings {
        seed_retry: Duration::from_millis(200),
        departure_grace: Duration::from_millis(100),
        leave_grace: Duration::from_millis(200),
    }
}

fn fast_quic_timings() -> QuicTimings {
    QuicTimings {
        keep_alive: Duration::from_millis(200),
        idle_timeout: Duration::from_secs(1),
    }
}

/// A node over QUIC tuned for localhost.
fn quic(name: &str, addr: SocketAddr, tls: Tls) -> ClusterConfig {
    ClusterConfig::new(name, Quic::new(addr, tls).timings(fast_quic_timings()))
}

struct TestNode {
    cluster: Cluster,
    shutdown: zestors::runtime::Address<zestors_supervisor::SupervisorInterface>,
    handle: JoinHandle<Result<(), ClusterNodeError>>,
}

fn start(name: &str, addr: SocketAddr, seed: Option<(&str, SocketAddr)>) -> TestNode {
    let mut config = quic(name, addr, Tls::insecure_dev().unwrap())
        .timings(fast_timings())
        .link_timings(fast_link_timings())
        .foca_config(fast_foca_config());
    if let Some((seed_name, seed_addr)) = seed {
        config = config.seed(Seed::new(seed_name, seed_addr));
    }
    let node = ClusterNode::new(Supervisor::blueprint().rand_name(), config)
        .with_exit_delay(Duration::ZERO);
    TestNode {
        cluster: node.cluster(),
        shutdown: node.root_supervisor().address().clone(),
        handle: tokio::spawn(node.run()),
    }
}

/// Fails the test if `future` takes unreasonably long.
async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(30), future)
        .await
        .expect("Timed out")
}

async fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("Timed out waiting for: {what}"));
}

/// The first event that says `node` is gone.
async fn departure_of(rx: &mut broadcast::Receiver<ClusterEvent>, node: &str) -> ClusterEvent {
    tokio::time::timeout(Duration::from_secs(30), async {
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
    .unwrap_or_else(|_| panic!("Timed out waiting for {node} to be reported gone"))
}

#[tokio::test(flavor = "multi_thread")]
async fn nodes_discover_each_other_and_notice_departures() {
    let (a_addr, b_addr, c_addr) = (free_addr(), free_addr(), free_addr());

    let a = start("node-a", a_addr, None);
    let b = start("node-b", b_addr, Some(("node-a", a_addr)));
    let c = start("node-c", c_addr, Some(("node-a", a_addr)));

    for node in [&a, &b, &c] {
        within(node.cluster.wait_for_members(2)).await;
    }

    let mut a_events = a.cluster.subscribe();

    // A node that stops gracefully is reported as having left.
    c.shutdown.signal_shutdown();
    c.handle.await.unwrap().unwrap();
    assert!(matches!(
        departure_of(&mut a_events, "node-c").await,
        ClusterEvent::Left(_)
    ));
    eventually("a and b notice c left", || {
        a.cluster.members().len() == 1 && b.cluster.members().len() == 1
    })
    .await;

    // A node is identified by its name, not its address: restarted somewhere
    // else, it replaces its previous incarnation.
    let old_generation = c.cluster.local_member().generation;
    let c2_addr = free_addr();
    assert_ne!(c_addr, c2_addr);
    let c2 = start("node-c", c2_addr, Some(("node-a", a_addr)));
    eventually("c is back at its new address", || {
        a.cluster.members().len() == 2
            && c2.cluster.members().len() == 2
            && a.cluster.member(&NodeName::new("node-c")).map(|m| m.addr) == Some(c2_addr.into())
    })
    .await;
    assert!(c2.cluster.local_member().generation > old_generation);

    // A node that vanishes without saying goodbye is reported as failed.
    b.handle.abort();
    assert!(matches!(
        departure_of(&mut a_events, "node-b").await,
        ClusterEvent::Failed(_)
    ));
    eventually("a and c see only each other", || {
        a.cluster.members().len() == 1 && c2.cluster.members().len() == 1
    })
    .await;

    a.shutdown.signal_shutdown();
    c2.shutdown.signal_shutdown();
    let (a_exit, c2_exit) = tokio::join!(a.handle, c2.handle);
    a_exit.unwrap().unwrap();
    c2_exit.unwrap().unwrap();
}

struct Ca {
    cert: rcgen::Certificate,
    issuer: rcgen::Issuer<'static, rcgen::KeyPair>,
}

impl Ca {
    fn new() -> Self {
        let key = rcgen::KeyPair::generate().unwrap();
        let mut params = rcgen::CertificateParams::new(vec![]).unwrap();
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let cert = params.self_signed(&key).unwrap();
        let issuer = rcgen::Issuer::new(params, key);
        Self { cert, issuer }
    }

    /// TLS for a node whose certificate is valid for `cert_name`.
    fn tls(&self, cert_name: &str) -> Tls {
        let key = rcgen::KeyPair::generate().unwrap();
        let params = rcgen::CertificateParams::new(vec![cert_name.to_string()]).unwrap();
        let cert = params.signed_by(&key, &self.issuer).unwrap();
        Tls::from_pem(
            self.cert.pem().as_bytes(),
            cert.pem().as_bytes(),
            key.serialize_pem().as_bytes(),
        )
        .unwrap()
    }
}

fn start_with(config: ClusterConfig) -> TestNode {
    let node = ClusterNode::new(Supervisor::blueprint().rand_name(), config)
        .with_exit_delay(Duration::ZERO);
    TestNode {
        cluster: node.cluster(),
        shutdown: node.root_supervisor().address().clone(),
        handle: tokio::spawn(node.run()),
    }
}

fn config(name: &str, addr: SocketAddr, tls: Tls) -> ClusterConfig {
    quic(name, addr, tls)
        .timings(fast_timings())
        .link_timings(fast_link_timings())
        .foca_config(fast_foca_config())
}

/// A node is who its certificate says. One that is configured with another
/// name than its certificate carries doesn't start, and can't get in as anyone.
#[tokio::test(flavor = "multi_thread")]
async fn node_cannot_use_a_name_its_certificate_lacks() {
    let ca = Ca::new();
    let (a_addr, b_addr, m_addr) = (free_addr(), free_addr(), free_addr());

    let a = start_with(config("node-a", a_addr, ca.tls("node-a")));

    // Mallory holds a valid certificate, but for "mallory", and calls herself node-b.
    let mallory =
        start_with(config("node-b", m_addr, ca.tls("mallory")).seed(Seed::new("node-a", a_addr)));
    let error = within(mallory.handle)
        .await
        .unwrap()
        .expect_err("Mallory can't start");
    assert!(matches!(error, ClusterNodeError::Backend(_)), "{error}");

    // The honest node-b2 is unaffected and can join.
    let b =
        start_with(config("node-b2", b_addr, ca.tls("node-b2")).seed(Seed::new("node-a", a_addr)));
    eventually("the honest node joins", || {
        a.cluster.member(&NodeName::new("node-b2")).is_some()
    })
    .await;
    assert!(a.cluster.member(&NodeName::new("node-b")).is_none());

    for node in [&a, &b] {
        node.shutdown.signal_shutdown();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn status_follows_the_node_lifecycle() {
    let addr = free_addr();
    let node = ClusterNode::new(
        Supervisor::blueprint().rand_name(),
        config("node-a", addr, Tls::insecure_dev().unwrap()),
    )
    .with_exit_delay(Duration::ZERO);
    let cluster = node.cluster();
    let shutdown = node.shutdown_handle();
    assert_eq!(cluster.status(), NodeStatus::Starting);

    let handle = tokio::spawn(node.run());
    within(cluster.wait_for_status(NodeStatus::Up)).await;

    shutdown.shutdown();
    handle.await.unwrap().unwrap();
    assert_eq!(cluster.status(), NodeStatus::Leaving);
}

/// With a generation store, a restart always has a higher generation than the
/// run before it, even if the store claims that run happened in the future
/// (as it would after the clock was set back).
#[tokio::test(flavor = "multi_thread")]
async fn generation_store_keeps_generations_growing() {
    let store = std::env::temp_dir().join(format!("zestors-generation-{}", std::process::id()));
    let future = 32_503_680_000_000u64; // The year 3000, in milliseconds.
    std::fs::write(&store, future.to_string()).unwrap();

    let node = ClusterNode::new(
        Supervisor::blueprint().rand_name(),
        config("node-a", free_addr(), Tls::insecure_dev().unwrap()).generation_store(&store),
    )
    .with_exit_delay(Duration::ZERO);
    let (cluster, shutdown) = (node.cluster(), node.shutdown_handle());
    let handle = tokio::spawn(node.run());
    within(cluster.wait_for_status(NodeStatus::Up)).await;

    let generation = cluster.local_member().generation;
    assert!(generation > future);
    assert_eq!(
        std::fs::read_to_string(&store).unwrap(),
        generation.to_string()
    );

    shutdown.shutdown();
    handle.await.unwrap().unwrap();
    let _ = std::fs::remove_file(&store);
}
