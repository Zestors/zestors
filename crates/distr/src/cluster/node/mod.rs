mod config;
mod error;

use super::{Cluster, Member, generation, membership::Membership};
use crate::{NodeAddr, backend::NodeIncarnation};
pub use config::{ClusterConfig, ClusterTimings, Seed};
pub use error::ClusterNodeError;
use std::net::SocketAddr;
use zestors_supervision::ChildSpec;
use zestors_supervisor::{Node, SupervisorBlueprint};

/// A [`Node`] that also joins a cluster.
///
/// It runs the same root supervisor with the same shutdown behavior as
/// [`Node`], and additionally takes part in a gossip-based membership
/// protocol over a [backend](crate::backend) (mutually authenticated QUIC with
/// `zestors-distr-quic`), and serves the messages registered in its
/// [`ClusterConfig`]. Observe the cluster and address actors through
/// [`ClusterNode::cluster`], taken before [`ClusterNode::run`].
///
/// ```no_run
/// use zestors::{prelude::*, supervisor::Supervisor};
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let tls = Tls::from_pem(
///     &std::fs::read("ca.pem")?,
///     &std::fs::read("node-a.pem")?,
///     &std::fs::read("node-a.key")?,
/// )?;
/// let config = ClusterConfig::new("node-a", Quic::new("0.0.0.0:7000".parse()?, tls))
///     .seed(Seed::new("node-b", "node-b.internal:7000"));
///
/// let node = ClusterNode::new(Supervisor::blueprint().rand_name(), config);
/// let cluster = node.cluster();
/// node.run().await?;
/// # drop(cluster);
/// # Ok(())
/// # }
/// ```
pub struct ClusterNode {
    node: Node,
    config: ClusterConfig,
    cluster: Cluster,
}

impl ClusterNode {
    /// Creates a [`ClusterNode`] for the root supervisor described by `spec`.
    pub fn new(spec: ChildSpec<SupervisorBlueprint>, config: ClusterConfig) -> Self {
        Self::from_node(Node::new(spec), config)
    }

    /// Adds cluster membership to an existing [`Node`].
    pub fn from_node(node: Node, mut config: ClusterConfig) -> Self {
        // The registry moves out of the config: it is fixed from here on.
        let handlers = std::mem::take(&mut config.handlers);
        let cluster = Cluster::new(
            Member {
                name: config.node_id.clone(),
                // Not known until the backend has started, unless configured.
                addr: config
                    .advertise
                    .clone()
                    .unwrap_or_else(|| NodeAddr::from(SocketAddr::from(([0, 0, 0, 0], 0)))),
                generation: 0,
            },
            config.call_timeout,
            config.lanes.get(),
            handlers,
        );
        Self {
            node,
            config,
            cluster,
        }
    }

    /// Returns the root supervisor's [`ChildSpec`].
    pub fn root_supervisor(&self) -> &ChildSpec<SupervisorBlueprint> {
        self.node.root_supervisor()
    }

    /// A handle for observing the cluster and for addressing actors in it.
    /// Cheap to clone and usable from anywhere, also before
    /// [`ClusterNode::run`] is called.
    pub fn cluster(&self) -> Cluster {
        self.cluster.clone()
    }

    /// Joins the cluster and runs the root supervisor until the node exits,
    /// like [`Node::run`]. On the way out the node announces its departure so
    /// peers see it leave immediately rather than time out.
    pub async fn run(self) -> Result<(), ClusterNodeError> {
        let Self {
            node,
            config,
            cluster,
        } = self;

        let generation = generation::next(config.generation_store.as_deref())
            .map_err(ClusterNodeError::Generation)?;
        let options = config.membership_options();
        let (links, addr) = config
            .backend
            .start(
                NodeIncarnation::new(config.node_id.clone(), generation),
                config.link_timings.clone(),
            )
            .await
            .map_err(ClusterNodeError::Backend)?;
        let local_addr = config.advertise.clone().unwrap_or(addr);
        let membership = Membership::start(
            cluster.clone(),
            Member {
                name: config.node_id.clone(),
                addr: local_addr.clone(),
                generation,
            },
            options,
            links.clone(),
        )
        .await;
        tracing::info!(node = %config.node_id, addr = %local_addr, "Cluster node started");

        let messaging = cluster.start(links.clone());

        let result = node.run().await;

        // Stops taking messages and gives up on the calls still waiting, which
        // its `Drop` would also do if this node never got here.
        drop(messaging);
        membership.leave().await;
        links.shutdown().await;

        result.map_err(ClusterNodeError::from)
    }
}
