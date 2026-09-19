mod config;
mod error;

use super::{Cluster, Member, NodeAddr, backend::LocalNode, generation, membership::Membership};
use crate::messaging::Remote;
pub use config::ClusterConfig;
pub use error::ClusterNodeError;
use std::{net::SocketAddr, time::Duration};
use zestors_supervision::{ChildSpec, RestartIntensity};
use zestors_supervisor::{Node, NodeShutdown, SupervisorBlueprint};

/// A [`Node`] that also joins a cluster.
///
/// It runs the same root supervisor with the same restart and shutdown
/// behavior as [`Node`], and additionally takes part in a gossip-based
/// membership protocol over mutually authenticated QUIC. Observe the cluster
/// through [`ClusterNode::cluster`].
pub struct ClusterNode {
    node: Node,
    config: ClusterConfig,
    cluster: Cluster,
    remote: Remote,
}

impl ClusterNode {
    /// Creates a [`ClusterNode`] for the root supervisor described by `spec`.
    pub fn new(spec: ChildSpec<SupervisorBlueprint>, config: ClusterConfig) -> Self {
        Self::from_node(Node::new(spec), config)
    }

    /// Adds cluster membership to an existing [`Node`].
    pub fn from_node(node: Node, config: ClusterConfig) -> Self {
        let cluster = Cluster::new(Member {
            node: config.node_id.clone(),
            // Not known until the backend has started, unless configured.
            addr: config
                .advertise
                .clone()
                .unwrap_or_else(|| NodeAddr::from(SocketAddr::from(([0, 0, 0, 0], 0)))),
            generation: 0,
        });
        let remote = Remote::new(cluster.clone(), config.call_timeout);
        Self {
            node,
            config,
            cluster,
            remote,
        }
    }

    /// See [`Node::with_restart_intensity`].
    pub fn with_restart_intensity(mut self, intensity: RestartIntensity) -> Self {
        self.node = self.node.with_restart_intensity(intensity);
        self
    }

    /// See [`Node::with_exit_delay`].
    pub fn with_exit_delay(mut self, delay: Duration) -> Self {
        self.node = self.node.with_exit_delay(delay);
        self
    }

    /// See [`Node::shutdown_handle`]: shuts the node down gracefully at any time,
    /// also before [`ClusterNode::run`] has started it.
    pub fn shutdown_handle(&self) -> NodeShutdown {
        self.node.shutdown_handle()
    }

    /// Returns the root supervisor's [`ChildSpec`].
    pub fn root_supervisor(&self) -> &ChildSpec<SupervisorBlueprint> {
        self.node.root_supervisor()
    }

    /// A handle for messaging actors on other nodes, and for registering the
    /// messages this node accepts from them. Cheap to clone and usable from
    /// anywhere, also before [`ClusterNode::run`] is called.
    pub fn remote(&self) -> Remote {
        self.remote.clone()
    }

    /// A handle for observing the cluster. Cheap to clone and usable from
    /// anywhere, also before [`ClusterNode::run`] is called.
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
            remote,
        } = self;

        let generation = generation::next(config.generation_store.as_deref())
            .map_err(ClusterNodeError::Generation)?;
        let options = config.membership_options();
        let (links, addr) = config
            .backend
            .start(
                LocalNode {
                    id: config.node_id.clone(),
                    generation,
                },
                config.timings.clone(),
            )
            .await
            .map_err(ClusterNodeError::Backend)?;
        let local_addr = config.advertise.clone().unwrap_or(addr);
        let membership = Membership::start(
            cluster,
            Member {
                node: config.node_id.clone(),
                addr: local_addr.clone(),
                generation,
            },
            options,
            links.clone(),
        )
        .await;
        tracing::info!(node = %config.node_id, addr = %local_addr, "Cluster node started");

        let messaging = remote.start(links.clone());

        let result = node.run().await;

        messaging.stop();
        membership.leave().await;
        links.shutdown(config.timings.shutdown_grace).await;

        result.map_err(ClusterNodeError::from)
    }
}
