use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use super::{challenge::VerifiedStream, local_node::LocalNode, node::Node, NodeId};

#[derive(Debug)]
pub(crate) struct Cluster {
    nodes: RwLock<HashMap<NodeId, Node>>,
}

impl Cluster {
    pub(crate) fn new() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
        }
    }

    pub(crate) fn get_node(&self, id: NodeId) -> Option<Node> {
        self.nodes.read().unwrap().get(&id).map(|node| node.clone())
    }

    pub(crate) fn spawn_node(
        &self,
        local_node: &LocalNode,
        node_id: NodeId,
        stream: VerifiedStream,
    ) -> Result<Node, AddNodeError> {
        let addr = stream.peer_addr()?;

        // If the node to be added has the same id as this node, reject
        if local_node.node_id() == node_id {
            return Err(AddNodeError::NodeIdIsLocalNode);
        }

        // Get wlock
        let mut nodes = self.nodes.write().unwrap();

        // If the node is already registered, reject
        if nodes.contains_key(&node_id) {
            return Err(AddNodeError::NodeIdAlreadyRegistered);
        }

        // Spawn the node
        let node = Node::spawn(node_id, addr, stream, local_node.clone());

        // insert it into the nodes
        nodes.insert(node_id, node.clone());

        drop(nodes);

        Ok(node)
    }

    pub(crate) fn remove_node(&self, node_id: &NodeId) -> Option<Node> {
        self.nodes.write().unwrap().remove(&node_id)
    }
}

#[derive(Debug)]
pub(crate) enum AddNodeError {
    NodeIdAlreadyRegistered,
    NodeIdIsLocalNode,
    Io(std::io::Error),
}

impl From<std::io::Error> for AddNodeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
