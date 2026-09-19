//! Distributed clustering for `zestors`.
//!
//! [`ClusterNode`] is a drop-in replacement for the supervisor crate's `Node`
//! that also joins a cluster: membership is tracked with the SWIM gossip
//! protocol ([`foca`]). Messages between nodes are carried by a
//! [`backend`]: mutually authenticated QUIC by default (the `quic` feature),
//! or any implementation of [`backend::Backend`].

#[allow(unused_imports)]
mod _prelude {
    pub use crate::*;
}

pub mod prelude {
    #[cfg(feature = "quic")]
    pub use crate::Tls;
    pub use crate::{
        Cluster, ClusterConfig, ClusterEvent, ClusterNode, ClusterNodeError, ClusterSnapshot,
        NodeStatus, Seed,
    };
}

mod stable_id;
pub use stable_id::*;

mod global_pid;
pub use global_pid::*;

mod cluster;
pub use cluster::backend;
#[cfg(feature = "sim")]
pub use cluster::sim;
pub use cluster::{
    Addr, Cluster, ClusterConfig, ClusterEvent, ClusterNode, ClusterNodeError, ClusterSnapshot,
    ClusterTimings, Member, NodeStatus, Seed,
};
#[cfg(feature = "quic")]
pub use cluster::{Tls, TlsError};
