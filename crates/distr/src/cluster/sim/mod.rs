//! Runs whole clusters in one process, on a simulated network.
//!
//! Nodes talk over an in-memory network instead of QUIC. Combined with
//! `#[tokio::test(start_paused = true)]` a cluster runs on virtual time:
//! seconds of failure detection take no real time, and a run repeats exactly
//! when the seeds are the same. The network can be slowed down and cut in two,
//! which real sockets on one machine can't do.
//!
//! It runs the real membership protocol; only the transport is replaced. What it
//! doesn't model: TLS, connection setup and reconnect backoff. Those are
//! covered by tests over real QUIC.
//!
//! ```no_run
//! # async fn example() {
//! use zestors_distr::sim::SimNetwork;
//!
//! let mut net = SimNetwork::new(1);
//! let a = net.start("node-a", "10.0.0.1:7000".parse().unwrap(), &[]).await;
//! let b = net
//!     .start("node-b", "10.0.0.2:7000".parse().unwrap(), &[("node-a", "10.0.0.1:7000".parse().unwrap())])
//!     .await;
//! a.cluster.wait_for_members(1).await;
//!
//! net.partition(&["node-a"], &["node-b"]);
//! a.cluster.wait_for_members(0).await;
//! # drop(b);
//! # }
//! ```

mod fabric;

use super::{
    Cluster, ClusterTimings, Member, Seed,
    membership::{Membership, Options},
};
use crate::NodeId;
use fabric::Fabric;
use rand::{SeedableRng, rngs::StdRng};
use std::{net::SocketAddr, num::NonZeroU32, sync::Arc, time::Duration};

/// A simulated network of cluster nodes.
pub struct SimNetwork {
    fabric: Fabric,
    seed: u64,
    generation: u64,
    foca: foca::Config,
    timings: ClusterTimings,
}

impl SimNetwork {
    /// A network whose random choices, and those of the nodes started on it,
    /// are all decided by `seed`.
    ///
    /// Nodes use the same membership settings as [`ClusterConfig`](crate::ClusterConfig)
    /// by default, see [`SimNetwork::foca_config_mut`].
    pub fn new(seed: u64) -> Self {
        Self {
            fabric: Fabric::new(seed),
            seed,
            generation: 0,
            foca: foca::Config::new_lan(NonZeroU32::new(32).unwrap()),
            timings: ClusterTimings::default(),
        }
    }

    /// The membership protocol settings of nodes started from now on.
    pub fn foca_config_mut(&mut self) -> &mut foca::Config {
        &mut self.foca
    }

    /// The timeouts and intervals of nodes started from now on. The simulated
    /// network only has a use for those of the membership layer.
    pub fn timings_mut(&mut self) -> &mut ClusterTimings {
        &mut self.timings
    }

    /// Every message takes `latency` plus up to `jitter` to arrive.
    pub fn set_latency(&self, latency: Duration, jitter: Duration) {
        self.fabric.set_latency(latency, jitter);
    }

    /// Cuts the links between every node named in `a` and every node named in `b`.
    pub fn partition(&self, a: &[&str], b: &[&str]) {
        self.fabric.partition(a, b);
    }

    /// Restores every link.
    pub fn heal(&self) {
        self.fabric.heal();
    }

    /// Starts the node `name` listening at `addr`, which contacts `seeds` to
    /// join. Addresses only need to be different from each other.
    ///
    /// Every start is a new incarnation: a node started again under the same
    /// name replaces the one before it, at the same or another address.
    pub async fn start(
        &mut self,
        name: &str,
        addr: SocketAddr,
        seeds: &[(&str, SocketAddr)],
    ) -> SimNode {
        self.generation += 1;
        let local = Member {
            node: NodeId::new(name),
            addr,
            generation: self.generation,
        };
        let cluster = Cluster::new(local.clone());
        let (net, events) = self.fabric.bind(name, addr, self.generation);
        let options = Options {
            foca: self.foca.clone(),
            seeds: seeds
                .iter()
                .map(|&(node, addr)| Seed::new(node, addr))
                .collect(),
            timings: self.timings.clone(),
            rng: StdRng::seed_from_u64(self.seed.wrapping_add(self.generation)),
        };
        let membership =
            Membership::start(cluster.clone(), local, options, Arc::new(net), events).await;
        SimNode {
            cluster,
            membership: Some(membership),
        }
    }
}

/// A node of a [`SimNetwork`].
pub struct SimNode {
    /// The node's view of the cluster.
    pub cluster: Cluster,
    /// `None` once the node has left or crashed.
    membership: Option<Membership>,
}

impl SimNode {
    /// Announces its departure to the cluster, then stops.
    ///
    /// # Panics
    /// If the node has already left or crashed.
    pub async fn leave(&mut self) {
        self.membership
            .take()
            .expect("Node is running")
            .leave()
            .await;
    }

    /// Stops without a word, like a crash: the rest of the cluster has to find
    /// out by itself.
    pub async fn crash(&mut self) {
        self.membership = None;
        // Lets the stopped membership task drop, taking the node off the network.
        tokio::task::yield_now().await;
    }
}
