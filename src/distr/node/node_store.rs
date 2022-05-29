use crate::{self as zestors, core::*, distr::*};
use log::warn;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock}, net::SocketAddr,
};

//------------------------------------------------------------------------------------------------
//  NodeStore
//------------------------------------------------------------------------------------------------

/// The NodeStore is a Cloneable store of all nodes registered on the Endpoint.
/// Whenever a Node exits, the EndpointActor makes sure to remove the Node from the
/// NodeStore.
#[derive(Clone, Debug)]
pub(crate) struct NodeStore(Arc<RwLock<HashMap<NodeId, Node>>>);

impl NodeStore {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(HashMap::new())))
    }

    /// Attempts to store a `SystemNode`. Fails if it's `SystemId` is already stored.
    pub fn store_node(&self, node: Node) -> Result<(), Node> {
        let mut wl = self.0.write().unwrap();

        match wl.contains_key(&node.id()) {
            true => Err(node),
            false => {
                let _ = wl.insert(node.id().clone(), node);
                Ok(())
            }
        }
    }

    /// Attempts to remove a `SystemNode`. Fails if there is no `SystemNode` stored under the
    /// `SystemId`.
    pub fn remove_node(&self, id: &NodeId) -> Result<Node, NodeStoreRemoveError> {
        let mut wl = self.0.write().unwrap();

        match wl.remove(&id) {
            Some(system_node) => Ok(system_node),
            None => Err(NodeStoreRemoveError),
        }
    }
}

//------------------------------------------------------------------------------------------------
//  InnerNodeStore
//------------------------------------------------------------------------------------------------

struct InnerNodeStore {
    nodes_by_id: HashMap<NodeId, Node>
}