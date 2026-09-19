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

mod tls;
pub use tls::*;

mod generation;

mod net;

mod quic;

#[cfg(test)]
mod sim;

mod membership;
pub use membership::Member;

mod cluster;
pub use cluster::*;

mod cluster_node;
pub use cluster_node::*;
