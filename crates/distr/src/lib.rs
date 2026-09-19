//! Distributed clustering for `zestors`.
//!
//! [`ClusterNode`] is a drop-in replacement for the supervisor crate's `Node`
//! that also joins a cluster: membership is tracked with the SWIM gossip
//! protocol ([`foca`]) carried over mutually authenticated QUIC ([`quinn`]).

#[allow(unused_imports)]
mod _prelude {
    pub use crate::*;
}

pub mod prelude {
    pub use crate::{
        Cluster, ClusterConfig, ClusterEvent, ClusterNode, ClusterNodeError, ClusterSnapshot,
        NodeStatus, Seed, Tls,
    };
}

mod stable_id;
pub use stable_id::*;

mod global_pid;
pub use global_pid::*;

mod cluster;
#[cfg(feature = "sim")]
pub use cluster::sim;
pub use cluster::{
    Cluster, ClusterConfig, ClusterEvent, ClusterNode, ClusterNodeError, ClusterSnapshot,
    ClusterTimings, Member, NodeStatus, Seed, Tls, TlsError,
};
