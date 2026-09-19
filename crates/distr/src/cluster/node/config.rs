use super::super::membership::Options;
use crate::{ClusterTimings, NodeId, Seed, Tls};
use rand::{SeedableRng, rngs::StdRng};
use std::{net::SocketAddr, num::NonZeroU32, path::PathBuf};

/// How a [`ClusterNode`](crate::ClusterNode) joins and behaves in a cluster.
#[derive(Debug)]
pub struct ClusterConfig {
    pub(super) node_id: NodeId,
    pub(super) bind: SocketAddr,
    pub(super) advertise: Option<SocketAddr>,
    pub(super) seeds: Vec<Seed>,
    pub(super) tls: Tls,
    pub(super) foca: Option<foca::Config>,
    pub(super) expected_size: NonZeroU32,
    pub(super) timings: ClusterTimings,
    pub(super) rng_seed: Option<u64>,
    pub(super) generation_store: Option<PathBuf>,
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
