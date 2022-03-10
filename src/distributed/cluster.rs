use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use crate::{actor::{Actor, ProcessId}, address::Address};

use super::{challenge::VerifiedStream, local_node::LocalNode,  NodeId, pid::{ProcessRef, NodeLocation}, node::{NodeActor, Node}};

#[derive(Debug)]
pub struct Cluster {
    nodes: RwLock<HashMap<NodeId, Node>>,
}

impl Cluster {
    pub(crate) fn new() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
        }
    }

    pub fn get_node(&self, id: NodeId) -> Option<Node> {
        self.nodes.read().unwrap().get(&id).map(|node| node.clone())
    }

    pub(crate) fn add_node(&self, local_node_id: NodeId, node: Node) -> Result<(), AddNodeError> {
        if local_node_id == node.node_id() {
            Err(AddNodeError::NodeIdIsLocalNode)
        } else {
            let mut nodes = self.nodes.write().unwrap();
            if nodes.contains_key(&node.node_id()) {
                Err(AddNodeError::NodeIdAlreadyRegistered)
            } else {
                let res = nodes.insert(node.node_id(), node);
                assert!(res.is_none());
                Ok(())
            }
        }
    }

    pub(crate) fn remove_node(&self, node_id: NodeId) -> Option<Node> {
        self.nodes.write().unwrap().remove(&node_id)
    }
}

//------------------------------------------------------------------------------------------------
//  FindProcess
//------------------------------------------------------------------------------------------------

pub struct FindProcessRequest<A: Actor> {
    receiver: oneshot::Receiver<Option<ProcessRef<A>>>
}

impl<A: Actor> FindProcessRequest<A> {
    pub fn new() -> (oneshot::Sender<Option<ProcessRef<A>>>, Self) {
        let (sender, receiver) = oneshot::channel();
        let req = Self {
            receiver
        };

        (sender, req)
    }
}



//------------------------------------------------------------------------------------------------
//  AddNodeError
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) enum AddNodeError {
    NodeIdAlreadyRegistered,
    NodeIdIsLocalNode,
}

// impl From<std::io::Error> for AddNodeError {
//     fn from(e: std::io::Error) -> Self {
//         Self::Io(e)
//     }
// }
