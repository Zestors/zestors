//! Clustering for `zestors`: nodes that find each other, and actors that message
//! each other across them.
//!
//! The [distributed mode chapters of the zestors book](https://zestors.github.io/zestors/distributed/overview.html)
//! explain how the pieces fit together, and what is and isn't guaranteed.
//!
//! - [`ClusterNode`] runs a program as a node: it runs a root supervisor, like
//!   `zestors_supervisor::Node`, and also joins a cluster, configured with a
//!   [`ClusterConfig`].
//! - [`Cluster`], from [`ClusterNode::cluster`], shows who is in the cluster and
//!   makes addresses. Membership is tracked with the SWIM gossip protocol
//!   ([`foca`]).
//! - [`ClusterAddress`] is an actor anywhere in the cluster, named by a
//!   [`GlobalName`] (`name@node`). Messages are sent to it with
//!   [`ClusterAccepts`], and it is operated on with [`ClusterActorOps`],
//!   whether the actor is on this node or another.
//! - [`RemoteMessage`] is a message that can cross the network: it has a
//!   [`StableId`] and can be [`Encode`]d and [`Decode`]d, which every serde type
//!   can. A node accepts the remote messages
//!   [registered](ClusterConfig::register) in its config.
//! - A [`backend`] carries the bytes between nodes. The QUIC backend with
//!   mutual TLS is in `zestors-distr-quic`.
//!
//! # Example
//!
//! Two nodes, one calling an actor on the other. They run on the in-process
//! `sim` network here; for a real deployment, pass a QUIC backend to
//! [`ClusterConfig::new`] instead.
//!
//! ```
//! use serde::{Deserialize, Serialize};
//! use zestors::{
//!     distr::sim::SimNetwork,
//!     interface::{Envelope, Interface, Message},
//!     prelude::*,
//!     runtime::spawn,
//!     supervisor::Supervisor,
//! };
//!
//! // A message that can cross the network: a stable id, and serde.
//! #[derive(Message, StableId, Serialize, Deserialize, Debug)]
//! #[msg(reply = u32, id = "1c3d5e7f-2a4b-4c6d-8e0f-a1b2c3d4e5f6")]
//! struct Double(u32);
//!
//! #[derive(Interface, Debug)]
//! enum CalcInterface {
//!     Double(Envelope<Double>),
//! }
//!
//! # #[tokio::main(flavor = "current_thread", start_paused = true)]
//! # async fn main() {
//! let net = SimNetwork::new(1);
//! let a = ClusterNode::new(
//!     Supervisor::blueprint().rand_name(),
//!     ClusterConfig::new("node-a", net.backend("10.0.0.1:7000")),
//! );
//! // node-b serves the actor, so it registers the message it accepts.
//! let b = ClusterNode::new(
//!     Supervisor::blueprint().rand_name(),
//!     ClusterConfig::new("node-b", net.backend("10.0.0.2:7000"))
//!         .seed(Seed::new("node-a", "10.0.0.1:7000"))
//!         .register::<Double>(),
//! );
//!
//! let _calc = spawn(Name::new_static("calc"), |mut inbox: Inbox<CalcInterface>| async move {
//!     while let Some(CalcInterface::Double(envelope)) = inbox.recv().await {
//!         let n = envelope.msg.0;
//!         let _ = envelope.reply(n * 2);
//!     }
//!     Ok(())
//! })
//! .unwrap();
//!
//! let cluster = a.cluster();
//! tokio::spawn(a.run());
//! tokio::spawn(b.run());
//! cluster.wait_for_members(1).await;
//!
//! let calc = cluster
//!     .address::<CalcInterface>(GlobalName::new("calc", "node-b"))
//!     .await
//!     .unwrap();
//! assert_eq!(calc.call(Double(21)).await.unwrap(), 42);
//! # }
//! ```
//!
//! # Features
//!
//! - `auto-register`: `ClusterConfig::auto_register`, which registers every
//!   remote message in the binary.
//! - `sim`: the `sim` module, for running whole clusters inside a test.
#![cfg_attr(docsrs, feature(doc_cfg))]

/// The commonly used items, re-exported by `zestors::prelude`.
pub mod prelude {
    pub use crate::{
        Cluster, ClusterAccepts, ClusterActorOps, ClusterAddress, ClusterConfig, ClusterEvent,
        ClusterNode, ClusterNodeError, ClusterReceipt as _, ClusterSnapshot, GlobalName,
        NodeStatus, RemoteMessage, RemoteRequest, RemoteSet, Seed,
    };
}

/// Collects a message type for `ClusterConfig::auto_register`, used by the
/// derive of `StableId`.
#[cfg(feature = "auto-register")]
#[macro_export]
#[doc(hidden)]
macro_rules! __auto_register {
    ($ty:ty) => {
        $crate::__private::inventory::submit! {
            $crate::__private::Registration::new(|handlers| {
                #[allow(unused_imports)]
                use $crate::__private::{IfNot as _, IfRemote as _};
                (&$crate::__private::Probe::<$ty>::new()).register(handlers);
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
    pub use crate::messaging::{Handlers, IfNot, IfRemote, Probe, Registration};
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
#[cfg_attr(docsrs, doc(cfg(feature = "sim")))]
pub use cluster::sim;
pub use cluster::{
    Cluster, ClusterConfig, ClusterEvent, ClusterNode, ClusterNodeError, ClusterSnapshot,
    ClusterTimings, Member, NodeStatus, Seed,
};
pub use link::LinkTimings;
pub use messaging::{
    ActorInfo, AddressError, CastFailure, ClusterAccepts, ClusterActorOps, ClusterActorRef,
    ClusterAddress, ClusterCallError, ClusterCallOptions, ClusterCastError, ClusterOpError,
    ClusterReceipt, ClusterReply, ClusterReplyError, Decode, DecodeError, Encode, EncodeError,
    RemoteError, RemoteMessage, RemoteRequest, RemoteSet,
};
