//! The in-memory network behind [`SimNetwork`](super::SimNetwork).
//!
//! A [`Fabric`] is a network that any number of simulated nodes are bound to.
//! Frames travel between them as spawned tasks that sleep for the link's
//! latency, so under `#[tokio::test(start_paused = true)]` a whole cluster runs
//! on virtual time.
//!
//! The fabric mirrors what [`Transport`](crate::cluster::quic) promises and
//! nothing more: best-effort delivery, dialing by address with the node name
//! checked, and [`Event::Unreachable`] / [`Event::Reachable`] after repeated
//! failures. It adds what real networks do to tests: latency, and partitions.
//! Frames may overtake each other, as they may on QUIC.

use crate::{
    Member, NodeId,
    cluster::net::{Event, Frame, Incoming, Net},
};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;

/// How many sends in a row must fail before a peer is reported unreachable.
const UNREACHABLE_AFTER: u32 = 3;

/// A simulated network.
#[derive(Clone)]
pub(super) struct Fabric {
    inner: Arc<FabricInner>,
}

struct FabricInner {
    state: Mutex<State>,
}

struct State {
    /// The nodes currently listening, by the address they listen on.
    listeners: HashMap<SocketAddr, Listener>,
    /// Pairs of nodes that can't reach each other, in both directions.
    partitions: HashSet<(NodeId, NodeId)>,
    latency: Duration,
    jitter: Duration,
    rng: StdRng,
    next_instance: u64,
}

struct Listener {
    node: NodeId,
    /// Tells a stale registration from the one that replaced it.
    instance: u64,
    events: mpsc::Sender<Event>,
}

impl Fabric {
    /// A network whose random choices are decided by `seed`.
    pub(super) fn new(seed: u64) -> Self {
        Self {
            inner: Arc::new(FabricInner {
                state: Mutex::new(State {
                    listeners: HashMap::new(),
                    partitions: HashSet::new(),
                    latency: Duration::from_millis(1),
                    jitter: Duration::ZERO,
                    rng: StdRng::seed_from_u64(seed),
                    next_instance: 0,
                }),
            }),
        }
    }

    /// Every frame takes `latency` plus up to `jitter` to arrive.
    pub(super) fn set_latency(&self, latency: Duration, jitter: Duration) {
        let mut state = self.state();
        state.latency = latency;
        state.jitter = jitter;
    }

    /// Cuts the links between every node in `a` and every node in `b`.
    pub(super) fn partition(&self, a: &[&str], b: &[&str]) {
        let mut state = self.state();
        for &x in a {
            for &y in b {
                state.partitions.insert((NodeId::new(x), NodeId::new(y)));
                state.partitions.insert((NodeId::new(y), NodeId::new(x)));
            }
        }
    }

    /// Restores every link.
    pub(super) fn heal(&self) {
        self.state().partitions.clear();
    }

    /// Starts `node` listening on `addr`, replacing whatever listened there.
    pub(super) fn bind(
        &self,
        node: &str,
        addr: SocketAddr,
        generation: u64,
    ) -> (SimNet, mpsc::Receiver<Event>) {
        let (events_tx, events_rx) = mpsc::channel(1024);
        let mut state = self.state();
        let instance = state.next_instance;
        state.next_instance += 1;
        state.listeners.insert(
            addr,
            Listener {
                node: NodeId::new(node),
                instance,
                events: events_tx.clone(),
            },
        );
        let net = SimNet {
            fabric: self.clone(),
            local: NodeId::new(node),
            addr,
            generation,
            instance,
            events: events_tx,
            peers: Mutex::new(HashMap::new()),
        };
        (net, events_rx)
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner.state.lock().expect("Not poisoned")
    }
}

/// How this node is faring in reaching one peer.
#[derive(Default)]
struct PeerHealth {
    failures: u32,
    reported_unreachable: bool,
}

/// One node's handle to the [`Fabric`]. Dropping it takes the node off the
/// network, like a crash.
pub(super) struct SimNet {
    fabric: Fabric,
    local: NodeId,
    addr: SocketAddr,
    generation: u64,
    instance: u64,
    events: mpsc::Sender<Event>,
    peers: Mutex<HashMap<NodeId, PeerHealth>>,
}

impl SimNet {
    /// Records the outcome of a send and reports a change in reachability.
    fn record(&self, to: &NodeId, delivered: bool) {
        let mut peers = self.peers.lock().expect("Not poisoned");
        let health = peers.entry(to.clone()).or_default();
        if delivered {
            health.failures = 0;
            if std::mem::take(&mut health.reported_unreachable) {
                let _ = self.events.try_send(Event::Reachable(to.clone()));
            }
        } else {
            health.failures += 1;
            if health.failures >= UNREACHABLE_AFTER && !health.reported_unreachable {
                health.reported_unreachable = true;
                let _ = self.events.try_send(Event::Unreachable(to.clone()));
            }
        }
    }
}

impl Net for SimNet {
    fn send(&self, to: &Member, frame: Frame) {
        let (events, delay) = {
            let mut state = self.fabric.state();
            // Like dialing an address: whoever answers there must be the node
            // we mean, and reachable from here.
            let events = state
                .listeners
                .get(&to.addr)
                .filter(|listener| listener.node == to.node)
                .filter(|_| {
                    !state
                        .partitions
                        .contains(&(self.local.clone(), to.node.clone()))
                })
                .map(|listener| listener.events.clone());
            let jitter = state.jitter.mul_f64(state.rng.random_range(0.0..=1.0));
            (events, state.latency + jitter)
        };

        let Some(events) = events else {
            self.record(&to.node, false);
            return;
        };
        self.record(&to.node, true);

        let incoming = Incoming {
            from: self.local.clone(),
            generation: self.generation,
            frame,
        };
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = events.send(Event::Received(incoming)).await;
        });
    }

    fn forget(&self, node: &NodeId, _generation: u64) {
        self.peers.lock().expect("Not poisoned").remove(node);
    }
}

impl Drop for SimNet {
    fn drop(&mut self) {
        let mut state = self.fabric.state();
        if state
            .listeners
            .get(&self.addr)
            .is_some_and(|listener| listener.instance == self.instance)
        {
            state.listeners.remove(&self.addr);
        }
    }
}
