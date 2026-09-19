use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt::Display;
use zestors_runtime::Pid;

/// The name of a node in a distributed system.
///
/// Like Erlang's node atom, a `NodeId` names a node rather than locating it:
/// how a name maps to a network address is resolved by the connection layer, so
/// nodes can move without invalidating any [`GlobalPid`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(SmolStr);

impl NodeId {
    /// Creates a `NodeId` from any string-like name.
    pub fn new(name: impl Into<SmolStr>) -> Self {
        Self(name.into())
    }

    /// The name of the node.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for NodeId {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl From<String> for NodeId {
    fn from(name: String) -> Self {
        Self::new(name)
    }
}

/// A [`Pid`] qualified by the node it lives on, valid anywhere in the system.
///
/// Since a `Pid` is a stable name that is reused when an actor restarts, a
/// `GlobalPid` carries no incarnation counter (unlike Erlang's `creation`): it
/// keeps resolving to whichever actor currently holds that name on that node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GlobalPid {
    pid: Pid,
    node: NodeId,
}

impl GlobalPid {
    pub fn new(pid: impl Into<Pid>, node: impl Into<NodeId>) -> Self {
        Self {
            pid: pid.into(),
            node: node.into(),
        }
    }

    /// The node this pid lives on.
    pub fn node(&self) -> &NodeId {
        &self.node
    }

    /// The pid within its node.
    pub fn pid(&self) -> &Pid {
        &self.pid
    }

    pub fn into_parts(self) -> (Pid, NodeId) {
        (self.pid, self.node)
    }
}

/// Formats as `pid@node`, like an Erlang registered name.
impl Display for GlobalPid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.pid, self.node)
    }
}
