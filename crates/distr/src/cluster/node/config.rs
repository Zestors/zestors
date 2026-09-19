use crate::{
    LinkTimings, NodeAddr, NodeId, backend::Backend, cluster::membership::Options, link::Starter,
};
use rand::{SeedableRng, rngs::StdRng};
use std::{num::NonZeroU32, path::PathBuf, time::Duration};

/// How a [`ClusterNode`](crate::ClusterNode) joins and behaves in a cluster.
pub struct ClusterConfig {
    pub(super) node_id: NodeId,
    pub(super) backend: Starter,
    pub(super) advertise: Option<NodeAddr>,
    pub(super) seeds: Vec<Seed>,
    pub(super) foca: Option<foca::Config>,
    pub(super) expected_size: NonZeroU32,
    pub(super) timings: ClusterTimings,
    pub(super) link_timings: LinkTimings,
    pub(super) rng_seed: Option<u64>,
    pub(super) generation_store: Option<PathBuf>,
    pub(super) call_timeout: Duration,
}

impl ClusterConfig {
    /// Configures a node named `node_id` that talks to the others over
    /// `backend`, for example the QUIC backend of `zestors-distr-quic`.
    pub fn new(node_id: impl Into<NodeId>, backend: impl Backend) -> Self {
        Self {
            node_id: node_id.into(),
            backend: Starter::new(backend),
            advertise: None,
            seeds: Vec::new(),
            foca: None,
            expected_size: NonZeroU32::new(32).unwrap(),
            timings: ClusterTimings::default(),
            link_timings: LinkTimings::default(),
            rng_seed: None,
            generation_store: None,
            call_timeout: Duration::from_secs(30),
        }
    }

    /// Adds a node to contact when joining. A node with no seeds waits to be
    /// contacted by others.
    pub fn seed(mut self, seed: Seed) -> Self {
        self.seeds.push(seed);
        self
    }

    /// The address other nodes should use to reach this one, if it differs from
    /// the address the backend listens on (for example behind NAT or in a
    /// container).
    pub fn advertise(mut self, addr: impl Into<NodeAddr>) -> Self {
        self.advertise = Some(addr.into());
        self
    }

    /// Roughly how many nodes the cluster is expected to have. Tunes the
    /// default failure detection timings.
    pub fn expected_size(mut self, size: NonZeroU32) -> Self {
        self.expected_size = size;
        self
    }

    /// Sets the timeouts and intervals of the membership layer.
    pub fn timings(mut self, timings: ClusterTimings) -> Self {
        self.timings = timings;
        self
    }

    /// Sets the timeouts and intervals of the connections to other nodes.
    pub fn link_timings(mut self, timings: LinkTimings) -> Self {
        self.link_timings = timings;
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

    /// How long a call to an actor on another node waits for its reply before
    /// giving up, 30 seconds by default. See
    /// [`RemoteAccepts::call`](crate::RemoteAccepts::call).
    pub fn call_timeout(mut self, timeout: Duration) -> Self {
        self.call_timeout = timeout;
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

    /// What the membership protocol runs with.
    pub(super) fn membership_options(&self) -> Options {
        let foca = self.foca.clone().unwrap_or_else(|| {
            let mut foca = foca::Config::new_lan(self.expected_size);
            // Small enough to travel as a single QUIC datagram; anything larger
            // still gets through, but over a slower reliable stream.
            foca.max_packet_size = std::num::NonZeroUsize::new(1000).unwrap();
            foca
        });
        Options {
            foca,
            seeds: self.seeds.clone(),
            timings: self.timings.clone(),
            rng: match self.rng_seed {
                Some(seed) => StdRng::seed_from_u64(seed),
                None => rand::make_rng(),
            },
        }
    }
}

impl std::fmt::Debug for ClusterConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClusterConfig")
            .field("node_id", &self.node_id)
            .field("seeds", &self.seeds)
            .finish_non_exhaustive()
    }
}

/// A node to contact when joining the cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seed {
    /// The seed's node name, which its certificate must carry.
    pub node: NodeId,
    pub addr: NodeAddr,
}

impl Seed {
    pub fn new(node: impl Into<NodeId>, addr: impl Into<NodeAddr>) -> Self {
        Self {
            node: node.into(),
            addr: addr.into(),
        }
    }
}

/// The timeouts and intervals of the membership layer.
///
/// How quickly failures are *detected* is decided by the membership protocol's
/// own settings instead, see [`ClusterConfig::foca_config`](crate::ClusterConfig::foca_config).
/// The defaults suit real networks; shorten them for local tests. For the
/// connections underneath see [`LinkTimings`](crate::LinkTimings).
#[derive(Debug, Clone)]
pub struct ClusterTimings {
    /// How often the seeds are contacted again while this node knows of no
    /// other member.
    pub seed_retry: Duration,
    /// How long a node declared down by the failure detector may still turn out
    /// to have said goodbye, before it is reported as failed.
    pub departure_grace: Duration,
    /// How long to wait for the membership protocol to announce our departure.
    pub leave_grace: Duration,
}

impl Default for ClusterTimings {
    fn default() -> Self {
        Self {
            seed_retry: Duration::from_secs(5),
            departure_grace: Duration::from_millis(300),
            leave_grace: Duration::from_secs(1),
        }
    }
}
