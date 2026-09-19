use crate::NodeId;
use std::{net::SocketAddr, time::Duration};

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
/// own settings instead, see [`ClusterConfig::foca_config`](crate::ClusterConfig::foca_config). The defaults suit
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
