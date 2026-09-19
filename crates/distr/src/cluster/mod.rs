//! Which nodes make up the cluster, and the machinery that keeps it up to date.
//!
//! The submodules are layered: [`node`] runs a node, [`membership`] decides who
//! is in the cluster, [`link`] carries its messages, and [`backend`] is the
//! network underneath. Layers share what they need with their siblings through `pub(super)`
//! items, which are visible in this module and nowhere else. The public API is
//! re-exported from here.

mod addr;
pub mod backend;
mod config;
mod generation;
mod link;
mod member;
mod membership;
mod node;
mod updates;

#[cfg(feature = "sim")]
pub mod sim;

pub use addr::Addr;
#[cfg(feature = "quic")]
pub use backend::{Tls, TlsError};
pub use config::{ClusterTimings, Seed};
pub use member::Member;
pub use node::{ClusterConfig, ClusterNode, ClusterNodeError};

use crate::NodeId;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};
use tokio::sync::{broadcast, watch};

/// A change in which nodes are part of the cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClusterEvent {
    /// A node joined, or was rediscovered after being declared down.
    Up(Member),
    /// A node announced that it was shutting down cleanly.
    Left(Member),
    /// A node was declared down by the failure detector without saying
    /// goodbye: it crashed or became unreachable.
    ///
    /// A node that restarts before being noticed is reported as `Failed` for
    /// its previous incarnation followed by `Up` for the new one.
    Failed(Member),
    /// This node can't connect to a node that is still considered up.
    ///
    /// The node stays a member: this is only this node's view, and the failure
    /// detector still decides whether it is down. It is followed by
    /// [`ClusterEvent::Reachable`] if the node answers again, or by `Failed`
    /// or `Left` if it goes away.
    Unreachable(Member),
    /// A node reported [`ClusterEvent::Unreachable`] can be connected to again.
    Reachable(Member),
}

impl ClusterEvent {
    /// The node this event is about.
    pub fn member(&self) -> &Member {
        match self {
            ClusterEvent::Up(member)
            | ClusterEvent::Left(member)
            | ClusterEvent::Failed(member)
            | ClusterEvent::Unreachable(member)
            | ClusterEvent::Reachable(member) => member,
        }
    }
}

/// What this node is doing, as reported by [`Cluster::status`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    /// Not yet running: [`ClusterNode::run`](crate::ClusterNode::run) hasn't
    /// been called or hasn't finished starting.
    Starting,
    /// Taking part in the cluster.
    Up,
    /// Announcing its departure and shutting down.
    Leaving,
    /// Declared down by the cluster and unable to rejoin. Its view of the
    /// cluster is no longer maintained and it should be restarted.
    Defunct,
}

/// The members of a cluster at one moment, from
/// [`Cluster::subscribe_with_snapshot`].
#[derive(Debug, Clone)]
pub struct ClusterSnapshot {
    /// The other nodes considered up, in no particular order.
    pub members: Vec<Member>,
    unreachable: HashSet<NodeId>,
}

impl ClusterSnapshot {
    /// Whether `node` was a member that could be connected to.
    pub fn is_reachable(&self, node: &NodeId) -> bool {
        !self.unreachable.contains(node) && self.members.iter().any(|m| m.node == *node)
    }
}

/// What this node knows about the cluster. Kept under one lock so that readers
/// never see the parts disagree, and so that events can be published with a
/// change made under it (see `membership.rs`).
struct State {
    local: Member,
    members: HashMap<NodeId, Member>,
    /// Members in `members` that can't currently be connected to.
    unreachable: HashSet<NodeId>,
}

struct Shared {
    status: watch::Sender<NodeStatus>,
    state: RwLock<State>,
    events: broadcast::Sender<ClusterEvent>,
}

/// A cheaply cloneable handle to a running cluster, obtained from
/// [`ClusterNode::cluster`](crate::ClusterNode::cluster).
///
/// It is usable before the node has started, in which case it reports no
/// members.
#[derive(Clone)]
pub struct Cluster {
    shared: Arc<Shared>,
}

impl Cluster {
    fn new(local: Member) -> Self {
        Self {
            shared: Arc::new(Shared {
                status: watch::channel(NodeStatus::Starting).0,
                state: RwLock::new(State {
                    local,
                    members: HashMap::new(),
                    unreachable: HashSet::new(),
                }),
                events: broadcast::channel(256).0,
            }),
        }
    }

    /// This node, as the rest of the cluster knows it.
    pub fn local(&self) -> Member {
        self.state().local.clone()
    }

    fn state(&self) -> std::sync::RwLockReadGuard<'_, State> {
        self.shared.state.read().expect("Not poisoned")
    }

    fn set_local(&self, local: Member) {
        self.shared.state.write().expect("Not poisoned").local = local;
    }

    /// The other nodes currently considered up, in no particular order.
    pub fn members(&self) -> Vec<Member> {
        self.state().members.values().cloned().collect()
    }

    /// The other node named `node`, if it is currently up.
    pub fn member(&self, node: &NodeId) -> Option<Member> {
        self.state().members.get(node).cloned()
    }

    /// Whether `node` is a member that this node can currently connect to.
    /// False for nodes that aren't members, and for those reported as
    /// [`ClusterEvent::Unreachable`].
    pub fn is_reachable(&self, node: &NodeId) -> bool {
        let state = self.state();
        state.members.contains_key(node) && !state.unreachable.contains(node)
    }

    /// Subscribes to membership changes.
    ///
    /// Receivers that fall too far behind miss events; resynchronize with
    /// [`Cluster::subscribe_with_snapshot`] or [`Cluster::members`]. To also
    /// know who the members are right now, use `subscribe_with_snapshot`: calling
    /// `members()` and `subscribe()` separately can miss or double count a change
    /// in between.
    pub fn subscribe(&self) -> broadcast::Receiver<ClusterEvent> {
        self.shared.events.subscribe()
    }

    /// The current members together with a subscription to every change after
    /// them: each change is either in the snapshot or delivered as an event,
    /// never both and never neither.
    pub fn subscribe_with_snapshot(&self) -> (ClusterSnapshot, broadcast::Receiver<ClusterEvent>) {
        // Changes are published under the state lock (see `membership.rs`).
        let state = self.state();
        let events = self.shared.events.subscribe();
        let snapshot = ClusterSnapshot {
            members: state.members.values().cloned().collect(),
            unreachable: state.unreachable.clone(),
        };
        (snapshot, events)
    }

    /// Waits until `condition` holds for the current members, and returns them.
    /// Returns immediately if it already does.
    ///
    /// The condition is checked against every change, so it may run often;
    /// keep it cheap. Never resolves if the condition never holds; wrap it in a
    /// timeout when that matters.
    pub async fn wait_until(&self, mut condition: impl FnMut(&[Member]) -> bool) -> Vec<Member> {
        let (snapshot, mut events) = self.subscribe_with_snapshot();
        let mut members = snapshot.members;
        loop {
            if condition(&members) {
                return members;
            }
            match events.recv().await {
                Ok(_) => {}
                // Missed some changes: start over from the current state.
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => {
                    unreachable!("The cluster holds the sender")
                }
            }
            members = self.members();
        }
    }

    /// Waits until exactly `count` other nodes are considered up.
    pub async fn wait_for_members(&self, count: usize) -> Vec<Member> {
        self.wait_until(|members| members.len() == count).await
    }

    /// What this node is currently doing.
    pub fn status(&self) -> NodeStatus {
        *self.shared.status.borrow()
    }

    /// Waits until [`Cluster::status`] is `status`.
    pub async fn wait_for_status(&self, status: NodeStatus) {
        let mut rx = self.shared.status.subscribe();
        let _ = rx.wait_for(|current| *current == status).await;
    }

    fn set_status(&self, status: NodeStatus) {
        self.shared.status.send_if_modified(|current| {
            // Nothing follows Defunct or Leaving except leaving the cluster.
            let allowed = match (*current, status) {
                (NodeStatus::Defunct, _) => false,
                (NodeStatus::Leaving, next) => next != NodeStatus::Up,
                _ => true,
            };
            let changed = allowed && *current != status;
            if changed {
                *current = status;
            }
            changed
        });
    }
}

impl std::fmt::Debug for Cluster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cluster")
            .field("local", &self.local())
            .field("members", &self.members())
            .finish()
    }
}
