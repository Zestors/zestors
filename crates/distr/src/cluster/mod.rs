//! Which nodes make up the cluster, and the machinery that keeps it up to date.
//!
//! [`node`] runs a node, [`membership`] decides who is in the cluster and
//! publishes it to the [`Cluster`] handle in [`handle`], and `sim` runs
//! whole clusters in one process. They talk to the network through
//! [`link`](crate::link). Siblings share what they need through `pub(super)`
//! items, which are visible in this module and nowhere else. The public API is
//! re-exported from here.

mod event;
mod generation;
mod handle;
mod member;
mod membership;
mod node;

#[cfg(feature = "sim")]
pub mod sim;

pub use event::{ClusterEvent, ClusterSnapshot, NodeStatus};
pub use handle::Cluster;
pub use member::Member;
pub use node::{ClusterConfig, ClusterNode, ClusterNodeError, ClusterTimings, Seed};
