//! Distributed clustering for `zestors`.
//!
//! [`ClusterNode`] is a drop-in replacement for the supervisor crate's `Node`
//! that also joins a cluster: membership is tracked with the SWIM gossip
//! protocol ([`foca`]). Messages between nodes are carried by a
//! [`backend`]: any implementation of [`backend::Backend`], such as the
//! mutually authenticated QUIC backend in the `zestors-distr-quic` crate.
//!
//! Actors on other nodes are messaged through a [`RemoteAddress`], from
//! [`ClusterNode::cluster`]; see [`Cluster`].

pub mod prelude {
    pub use crate::{
        Cluster, ClusterAddress, ClusterConfig, ClusterEvent, ClusterNode, ClusterNodeError,
        ClusterSnapshot, NodeStatus, RemoteAccepts, RemoteActorOps, RemoteAddress, RemoteMessage,
        RemoteRequest, Seed,
    };
}

/// Collects a message type for [`Cluster::auto_register`], used by the derive
/// of `StableId`.
#[cfg(feature = "auto-register")]
#[macro_export]
#[doc(hidden)]
macro_rules! __auto_register {
    ($ty:ty) => {
        $crate::__private::inventory::submit! {
            $crate::__private::Registration::new(|cluster| {
                #[allow(unused_imports)]
                use $crate::__private::{IfNot as _, IfRemote as _};
                (&$crate::__private::Probe::<$ty>::new()).register(cluster);
            })
        }
    };
}

/// Without the `auto-register` feature, there is nothing to collect.
#[cfg(not(feature = "auto-register"))]
#[macro_export]
#[doc(hidden)]
macro_rules! __auto_register {
    ($ty:ty) => {};
}

#[cfg(feature = "auto-register")]
#[doc(hidden)]
pub mod __private {
    pub use crate::messaging::{IfNot, IfRemote, Probe, Registration};
    pub use inventory;
}

mod stable_id;
pub use stable_id::*;

mod global_name;
pub use global_name::*;

pub use zestors_distr_backend as backend;
pub use zestors_distr_backend::{NodeAddr, NodeName};
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
    AddressError, CastFailure, ClusterAddress, Decode, DecodeError, Encode, EncodeError,
    RemoteAccepts, RemoteActorOps, RemoteActorRef, RemoteAddress, RemoteCallError,
    RemoteCallOptions, RemoteCastError, RemoteError, RemoteInfo, RemoteMessage, RemoteOpError,
    RemoteReceipt, RemoteReply, RemoteReplyError, RemoteRequest, Route,
};
