use crate::{Member, NodeId, Tls};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    num::NonZeroU32,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::sync::{broadcast, watch};
use zestors_supervisor::NodeError;

/// A change in which nodes are part of the cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClusterEvent {
    /// A node joined, or was rediscovered after being declared down.
    Up(Member),
    /// A node announced that it was shutting down cleanly.
    Left(Member),
    /// A node was declared down by the failure detector without saying
    /// goodbye: it crashed or became unreachable.
    ///
    /// A node that restarts before being noticed is reported as `Failed` for
    /// its previous incarnation followed by `Up` for the new one.
    Failed(Member),
    /// This node can't connect to a node that is still considered up.
    ///
    /// The node stays a member: this is only this node's view, and the failure
    /// detector still decides whether it is down. It is followed by
    /// [`ClusterEvent::Reachable`] if the node answers again, or by `Failed`
    /// or `Left` if it goes away.
    Unreachable(Member),
    /// A node reported [`ClusterEvent::Unreachable`] can be connected to again.
    Reachable(Member),
}

impl ClusterEvent {
    /// The node this event is about.
    pub fn member(&self) -> &Member {
        match self {
            ClusterEvent::Up(member)
            | ClusterEvent::Left(member)
            | ClusterEvent::Failed(member)
            | ClusterEvent::Unreachable(member)
            | ClusterEvent::Reachable(member) => member,
        }
    }
}

/// What this node is doing, as reported by [`Cluster::status`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    /// Not yet running: [`ClusterNode::run`](crate::ClusterNode::run) hasn't
    /// been called or hasn't finished starting.
    Starting,
    /// Taking part in the cluster.
    Up,
    /// Announcing its departure and shutting down.
    Leaving,
    /// Declared down by the cluster and unable to rejoin. Its view of the
    /// cluster is no longer maintained and it should be restarted.
    Defunct,
}

/// The members of a cluster at one moment, from
/// [`Cluster::subscribe_with_snapshot`].
#[derive(Debug, Clone)]
pub struct ClusterSnapshot {
    /// The other nodes considered up, in no particular order.
    pub members: Vec<Member>,
    unreachable: HashSet<NodeId>,
}

impl ClusterSnapshot {
    /// Whether `node` was a member that could be connected to.
    pub fn is_reachable(&self, node: &NodeId) -> bool {
        !self.unreachable.contains(node) && self.members.iter().any(|m| m.node == *node)
    }
}

/// What this node knows about the cluster. Kept under one lock so that readers
/// never see the parts disagree, and so that events can be published with a
/// change made under it (see `membership.rs`).
pub(crate) struct State {
    pub(crate) local: Member,
    pub(crate) members: HashMap<NodeId, Member>,
    /// Members in `members` that can't currently be connected to.
    pub(crate) unreachable: HashSet<NodeId>,
}

pub(crate) struct Shared {
    pub(crate) status: watch::Sender<NodeStatus>,
    pub(crate) state: RwLock<State>,
    pub(crate) events: broadcast::Sender<ClusterEvent>,
}

/// A cheaply cloneable handle to a running cluster, obtained from
/// [`ClusterNode::cluster`](crate::ClusterNode::cluster).
///
/// It is usable before the node has started, in which case it reports no
/// members.
#[derive(Clone)]
pub struct Cluster {
    pub(crate) shared: Arc<Shared>,
}

impl Cluster {
    pub(crate) fn new(local: Member) -> Self {
        Self {
            shared: Arc::new(Shared {
                status: watch::channel(NodeStatus::Starting).0,
                state: RwLock::new(State {
                    local,
                    members: HashMap::new(),
                    unreachable: HashSet::new(),
                }),
                events: broadcast::channel(256).0,
            }),
        }
    }

    /// This node, as the rest of the cluster knows it.
    pub fn local(&self) -> Member {
        self.state().local.clone()
    }

    fn state(&self) -> std::sync::RwLockReadGuard<'_, State> {
        self.shared.state.read().expect("Not poisoned")
    }

    pub(crate) fn set_local(&self, local: Member) {
        self.shared.state.write().expect("Not poisoned").local = local;
    }

    /// The other nodes currently considered up, in no particular order.
    pub fn members(&self) -> Vec<Member> {
        self.state().members.values().cloned().collect()
    }

    /// The other node named `node`, if it is currently up.
    pub fn member(&self, node: &NodeId) -> Option<Member> {
        self.state().members.get(node).cloned()
    }

    /// Whether `node` is a member that this node can currently connect to.
    /// False for nodes that aren't members, and for those reported as
    /// [`ClusterEvent::Unreachable`].
    pub fn is_reachable(&self, node: &NodeId) -> bool {
        let state = self.state();
        state.members.contains_key(node) && !state.unreachable.contains(node)
    }

    /// Subscribes to membership changes.
    ///
    /// Receivers that fall too far behind miss events; resynchronize with
    /// [`Cluster::subscribe_with_snapshot`] or [`Cluster::members`]. To also
    /// know who the members are right now, use `subscribe_with_snapshot`: calling
    /// `members()` and `subscribe()` separately can miss or double count a change
    /// in between.
    pub fn subscribe(&self) -> broadcast::Receiver<ClusterEvent> {
        self.shared.events.subscribe()
    }

    /// The current members together with a subscription to every change after
    /// them: each change is either in the snapshot or delivered as an event,
    /// never both and never neither.
    pub fn subscribe_with_snapshot(&self) -> (ClusterSnapshot, broadcast::Receiver<ClusterEvent>) {
        // Changes are published under the state lock (see `membership.rs`).
        let state = self.state();
        let events = self.shared.events.subscribe();
        let snapshot = ClusterSnapshot {
            members: state.members.values().cloned().collect(),
            unreachable: state.unreachable.clone(),
        };
        (snapshot, events)
    }

    /// Waits until `condition` holds for the current members, and returns them.
    /// Returns immediately if it already does.
    ///
    /// The condition is checked against every change, so it may run often;
    /// keep it cheap. Never resolves if the condition never holds; wrap it in a
    /// timeout when that matters.
    pub async fn wait_until(&self, mut condition: impl FnMut(&[Member]) -> bool) -> Vec<Member> {
        let (snapshot, mut events) = self.subscribe_with_snapshot();
        let mut members = snapshot.members;
        loop {
            if condition(&members) {
                return members;
            }
            match events.recv().await {
                Ok(_) => {}
                // Missed some changes: start over from the current state.
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => {
                    unreachable!("The cluster holds the sender")
                }
            }
            members = self.members();
        }
    }

    /// Waits until exactly `count` other nodes are considered up.
    pub async fn wait_for_members(&self, count: usize) -> Vec<Member> {
        self.wait_until(|members| members.len() == count).await
    }

    /// What this node is currently doing.
    pub fn status(&self) -> NodeStatus {
        *self.shared.status.borrow()
    }

    /// Waits until [`Cluster::status`] is `status`.
    pub async fn wait_for_status(&self, status: NodeStatus) {
        let mut rx = self.shared.status.subscribe();
        let _ = rx.wait_for(|current| *current == status).await;
    }

    pub(crate) fn set_status(&self, status: NodeStatus) {
        self.shared.status.send_if_modified(|current| {
            // Nothing follows Defunct or Leaving except leaving the cluster.
            let allowed = match (*current, status) {
                (NodeStatus::Defunct, _) => false,
                (NodeStatus::Leaving, next) => next != NodeStatus::Up,
                _ => true,
            };
            let changed = allowed && *current != status;
            if changed {
                *current = status;
            }
            changed
        });
    }
}

impl std::fmt::Debug for Cluster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cluster")
            .field("local", &self.local())
            .field("members", &self.members())
            .finish()
    }
}

/// A node to contact when joining the cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seed {
    /// The seed's node name, which its certificate must carry.
    pub node: NodeId,
    pub addr: SocketAddr,
}

impl Seed {
    pub fn new(node: impl Into<NodeId>, addr: SocketAddr) -> Self {
        Self {
            node: node.into(),
            addr,
        }
    }
}

/// The timeouts and intervals of the connection and membership layers.
///
/// How quickly failures are *detected* is decided by the membership protocol's
/// own settings instead, see [`ClusterConfig::foca_config`]. The defaults suit
/// real networks; shorten them for local tests.
#[derive(Debug, Clone)]
pub struct ClusterTimings {
    /// How long to wait for a connection to a peer to be established.
    pub connect_timeout: Duration,
    /// How long to wait for a new connection's identity exchange.
    pub handshake_timeout: Duration,
    /// How often idle connections send a keep-alive packet. Must be shorter
    /// than `idle_timeout`.
    pub keep_alive: Duration,
    /// How long a connection may go without any packets before it is dropped.
    pub idle_timeout: Duration,
    /// How long the sender task for a peer lingers without messages to send.
    pub peer_idle: Duration,
    /// How long to wait after a failed attempt to connect to a peer before trying
    /// again. Doubles with every further failure, up to `reconnect_backoff_max`.
    /// Messages for the peer in the meantime are dropped.
    pub reconnect_backoff_min: Duration,
    /// The longest pause between attempts to connect to an unreachable peer.
    pub reconnect_backoff_max: Duration,
    /// How often the seeds are contacted again while this node knows of no
    /// other member.
    pub seed_retry: Duration,
    /// How long a node declared down by the failure detector may still turn out
    /// to have said goodbye, before it is reported as failed.
    pub departure_grace: Duration,
    /// How long to wait for the membership protocol to announce our departure.
    pub leave_grace: Duration,
    /// How long to wait for queued messages to be delivered when shutting down.
    pub shutdown_grace: Duration,
}

impl Default for ClusterTimings {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            handshake_timeout: Duration::from_secs(5),
            keep_alive: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(30),
            peer_idle: Duration::from_secs(60),
            reconnect_backoff_min: Duration::from_millis(250),
            reconnect_backoff_max: Duration::from_secs(10),
            seed_retry: Duration::from_secs(5),
            departure_grace: Duration::from_millis(300),
            leave_grace: Duration::from_secs(1),
            shutdown_grace: Duration::from_secs(2),
        }
    }
}

/// How a [`ClusterNode`](crate::ClusterNode) joins and behaves in a cluster.
#[derive(Debug)]
pub struct ClusterConfig {
    pub(crate) node_id: NodeId,
    pub(crate) bind: SocketAddr,
    pub(crate) advertise: Option<SocketAddr>,
    pub(crate) seeds: Vec<Seed>,
    pub(crate) tls: Tls,
    pub(crate) foca: Option<foca::Config>,
    pub(crate) expected_size: NonZeroU32,
    pub(crate) timings: ClusterTimings,
    pub(crate) rng_seed: Option<u64>,
    pub(crate) generation_store: Option<PathBuf>,
}

impl ClusterConfig {
    /// Configures a node named `node_id` that listens on `bind`.
    ///
    /// The node name must be a valid DNS name; with [`Tls::from_pem`] it must
    /// also be a subject alternative name of the node's certificate.
    pub fn new(node_id: impl Into<NodeId>, bind: SocketAddr, tls: Tls) -> Self {
        Self {
            node_id: node_id.into(),
            bind,
            advertise: None,
            seeds: Vec::new(),
            tls,
            foca: None,
            expected_size: NonZeroU32::new(32).unwrap(),
            timings: ClusterTimings::default(),
            rng_seed: None,
            generation_store: None,
        }
    }

    /// Adds a node to contact when joining. A node with no seeds waits to be
    /// contacted by others.
    pub fn seed(mut self, seed: Seed) -> Self {
        self.seeds.push(seed);
        self
    }

    /// The address other nodes should use to reach this one, if it differs from
    /// the bind address (for example behind NAT or in a container).
    pub fn advertise(mut self, addr: SocketAddr) -> Self {
        self.advertise = Some(addr);
        self
    }

    /// Roughly how many nodes the cluster is expected to have. Tunes the
    /// default failure detection timings.
    pub fn expected_size(mut self, size: NonZeroU32) -> Self {
        self.expected_size = size;
        self
    }

    /// Sets the timeouts and intervals of the connection and membership layers.
    pub fn timings(mut self, timings: ClusterTimings) -> Self {
        self.timings = timings;
        self
    }

    /// Records the node's generation in `path`, so that it keeps growing across
    /// restarts even if the system clock is set back in between. Without it, a
    /// node started after such a step can be ignored by peers that still
    /// remember its earlier incarnation, until they forget it.
    ///
    /// The node fails to start if the file can't be read or written.
    pub fn generation_store(mut self, path: impl Into<PathBuf>) -> Self {
        self.generation_store = Some(path.into());
        self
    }

    /// Seeds the membership protocol's random choices (whom to probe and gossip
    /// with), so that its behavior is reproducible. Random by default.
    pub fn rng_seed(mut self, seed: u64) -> Self {
        self.rng_seed = Some(seed);
        self
    }

    /// Replaces the membership protocol settings entirely. This decides how
    /// quickly a crashed node is detected.
    ///
    /// Keep `max_packet_size` at or below roughly 1000 bytes: gossip that fits
    /// in a QUIC datagram is sent as one, larger packets fall back to a slower
    /// reliable stream.
    pub fn foca_config(mut self, config: foca::Config) -> Self {
        self.foca = Some(config);
        self
    }
}

/// Why a [`ClusterNode`](crate::ClusterNode) stopped running.
#[derive(Debug, thiserror::Error)]
pub enum ClusterNodeError {
    /// The node name is not a valid DNS name, so it can't be used with TLS.
    #[error("Invalid node name {0:?}: must be a valid DNS name")]
    InvalidNodeName(String),

    /// The QUIC endpoint could not be started.
    #[error("Failed to bind cluster endpoint: {0}")]
    Bind(#[source] std::io::Error),

    /// The generation could not be read from or recorded in the file given to
    /// [`ClusterConfig::generation_store`].
    #[error("Failed to use the generation store: {0}")]
    Generation(#[source] std::io::Error),

    /// The wrapped [`Node`](zestors_supervisor::Node) stopped with an error.
    #[error(transparent)]
    Node(#[from] NodeError),
}

impl ClusterNodeError {
    /// The process exit code this error should be reported with.
    pub fn exit_code(&self) -> i32 {
        match self {
            ClusterNodeError::Node(err) => err.exit_code(),
            ClusterNodeError::InvalidNodeName(_)
            | ClusterNodeError::Bind(_)
            | ClusterNodeError::Generation(_) => 1,
        }
    }
}
