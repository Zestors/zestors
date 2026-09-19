use crate::{NodeAddr, NodeId};
use foca::Identity;
use serde::{Deserialize, Serialize};

/// A node in the cluster, as known to the membership protocol.
///
/// A node is identified by its [`NodeId`] alone; `addr` is only where it can
/// currently be reached and may change between restarts. The `generation`
/// distinguishes successive incarnations of the same node, so a restarted node
/// replaces its previous incarnation (like Erlang's `creation`, but carried by
/// the node rather than by pids).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Member {
    /// The node's name; must be a DNS name matching its TLS certificate.
    pub node: NodeId,
    /// The address peers currently reach the node on.
    pub addr: NodeAddr,
    /// Increases every time the node restarts.
    pub generation: u64,
}

impl Identity for Member {
    type Addr = NodeId;

    fn renew(&self) -> Option<Self> {
        Some(Member {
            generation: self.generation + 1,
            ..self.clone()
        })
    }

    fn addr(&self) -> NodeId {
        self.node.clone()
    }

    fn win_addr_conflict(&self, adversary: &Self) -> bool {
        self.generation > adversary.generation
    }
}
