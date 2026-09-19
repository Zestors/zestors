use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt::Display;

/// The name of a node in a distributed system.
///
/// Like Erlang's node atom, a `NodeId` names a node rather than locating it:
/// how a name maps to a network address is resolved by the connection layer, so
/// nodes can move without invalidating any [`GlobalPid`](crate::GlobalPid).
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
