use super::Member;
use crate::NodeName;
use std::collections::HashSet;

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

/// What this node is doing, as reported by [`Cluster::status`](crate::Cluster::status).
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
/// [`Cluster::subscribe_with_snapshot`](crate::Cluster::subscribe_with_snapshot).
#[derive(Debug, Clone)]
pub struct ClusterSnapshot {
    /// The other nodes considered up, in no particular order.
    pub members: Vec<Member>,
    pub(super) unreachable: HashSet<NodeName>,
}

impl ClusterSnapshot {
    /// Whether `node` was a member that could be connected to.
    pub fn is_reachable(&self, node: &NodeName) -> bool {
        !self.unreachable.contains(node) && self.members.iter().any(|m| m.name == *node)
    }
}
