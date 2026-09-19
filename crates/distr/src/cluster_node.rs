use crate::{
    Cluster, ClusterConfig, ClusterNodeError, Member, NodeStatus,
    generation,
    membership::Membership,
    net::{Event, Net},
    quic::Transport,
};
use rand::{SeedableRng, rngs::StdRng};
use tokio::sync::mpsc;
use std::{
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};
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
            addr: config.advertise.unwrap_or(config.bind),
            generation: 0,
        });
        Self {
            node,
            config,
            cluster,
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
        } = self;

        // The node name doubles as the TLS server name when peers dial it.
        if quinn::rustls::pki_types::ServerName::try_from(config.node_id.as_str()).is_err() {
            return Err(ClusterNodeError::InvalidNodeName(
                config.node_id.as_str().to_owned(),
            ));
        }

        let generation = generation::next(config.generation_store.as_deref())
            .map_err(ClusterNodeError::Generation)?;
        let (transport, events) = Transport::bind(
            config.bind,
            &config.tls,
            config.node_id.clone(),
            generation,
            config.timings.clone(),
        )
        .map_err(ClusterNodeError::Bind)?;
        let transport = Arc::new(transport);

        let local_addr = match config.advertise {
            Some(addr) => addr,
            None => local_addr(&transport)?,
        };
        let membership = join(
            &cluster,
            &config,
            Member {
                node: config.node_id.clone(),
                addr: local_addr,
                generation,
            },
            transport.clone(),
            events,
        )
        .await;
        tracing::info!(node = %config.node_id, addr = %local_addr, "Cluster node started");

        let result = node.run().await;

        membership.leave().await;
        transport.shutdown(config.timings.shutdown_grace).await;

        result.map_err(ClusterNodeError::from)
    }
}

/// Makes `local` part of the cluster over `net`: marks `cluster` as up, starts
/// the membership protocol and announces to the seeds.
///
/// Everything a node does to join, short of creating the network it does so
/// over, so that tests can run it over a simulated one.
pub(crate) async fn join(
    cluster: &Cluster,
    config: &ClusterConfig,
    local: Member,
    net: Arc<dyn Net>,
    events: mpsc::Receiver<Event>,
) -> Membership {
    cluster.set_local(local);
    cluster.set_status(NodeStatus::Up);

    let seeds: Vec<Member> = config
        .seeds
        .iter()
        .map(|seed| Member {
            node: seed.node.clone(),
            addr: seed.addr,
            generation: 0,
        })
        .collect();
    let foca_config = config.foca.clone().unwrap_or_else(|| {
        let mut foca = foca::Config::new_lan(config.expected_size);
        // Small enough to travel as a single QUIC datagram; anything larger
        // still gets through, but over a slower reliable stream.
        foca.max_packet_size = std::num::NonZeroUsize::new(1000).unwrap();
        foca
    });
    let rng = match config.rng_seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => rand::make_rng(),
    };

    let membership = Membership::start(
        cluster.clone(),
        foca_config,
        seeds.clone(),
        config.timings.clone(),
        net,
        events,
        rng,
    );
    for seed in seeds {
        membership.announce(seed).await;
    }
    membership
}

fn local_addr(transport: &Transport) -> Result<SocketAddr, ClusterNodeError> {
    transport.local_addr().map_err(ClusterNodeError::Bind)
}
