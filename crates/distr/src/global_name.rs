use crate::NodeName;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use zestors_runtime::Name;

/// A [`Name`] qualified by the node it lives on, valid anywhere in the system.
///
/// Since a `Name` is stable and reused when an actor restarts, a
/// `GlobalName` carries no incarnation counter (unlike Erlang's `creation`): it
/// keeps resolving to whichever actor currently holds that name on that node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GlobalName {
    name: Name,
    node: NodeName,
}

impl GlobalName {
    /// The actor registered as `name` on the node `node`.
    pub fn new(name: impl Into<Name>, node: impl Into<NodeName>) -> Self {
        Self {
            name: name.into(),
            node: node.into(),
        }
    }

    /// The node this name lives on.
    pub fn node(&self) -> &NodeName {
        &self.node
    }

    /// The name within its node.
    pub fn name(&self) -> &Name {
        &self.name
    }

    /// Splits into the name and the node.
    pub fn into_parts(self) -> (Name, NodeName) {
        (self.name, self.node)
    }
}

/// Formats as `name@node`, like an Erlang registered name.
impl Display for GlobalName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.name, self.node)
    }
}
