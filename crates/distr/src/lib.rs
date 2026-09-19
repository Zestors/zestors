//! Distributed clustering for `zestors`.
//!
//! [`ClusterNode`] is a drop-in replacement for the supervisor crate's `Node`
//! that also joins a cluster: membership is tracked with the SWIM gossip
//! protocol ([`foca`]). Messages between nodes are carried by a
//! [`backend`]: any implementation of [`backend::Backend`], such as the
//! mutually authenticated QUIC backend in the `zestors-distr-quic` crate.
//!
//! Actors on other nodes are messaged through a [`RemoteAddress`], from
//! [`ClusterNode::remote`]; see [`Remote`].

pub mod prelude {
    pub use crate::{
        Cluster, ClusterConfig, ClusterEvent, ClusterNode, ClusterNodeError, ClusterSnapshot,
        NodeStatus, Remote, RemoteAccepts, RemoteAddress, RemoteMessage, RemoteRequest, Seed,
    };
}

mod stable_id;
pub use stable_id::*;

mod global_name;
pub use global_name::*;

pub use zestors_distr_backend as backend;
pub use zestors_distr_backend::{NodeAddr, NodeId};
mod cluster;
mod link;
mod messaging;
#[cfg(feature = "sim")]
pub use cluster::sim;
pub use cluster::{
    Cluster, ClusterConfig, ClusterEvent, ClusterNode, ClusterNodeError, ClusterSnapshot,
    ClusterTimings, Member, NodeStatus, Seed,
};
pub use link::LinkTimings;
pub use messaging::{
    Decode, DecodeError, Encode, EncodeError, Remote, RemoteAccepts, RemoteAddress,
    RemoteCallError, RemoteCallOptions, RemoteCastError, RemoteError, RemoteMessage, RemoteReceipt,
    RemoteReply, RemoteReplyError, RemoteRequest,
};
