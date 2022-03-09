use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use crate::{actor::Actor, address::Address};

use super::{challenge::VerifiedStream, local_node::LocalNode,  NodeId, ProcessId, pid::{Pid, NodeLocation}, node::NodeActor};

#[derive(Debug)]
pub struct Cluster {
    nodes: RwLock<HashMap<NodeId, Address<NodeActor>>>,
}

impl Cluster {
    pub(crate) fn new() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
        }
    }

    pub fn process_by_id<A: Actor>(&self, id: ProcessId) -> Option<Pid<A>> {


        todo!()
    }

    pub fn get_node(&self, id: NodeId) -> Option<()> {
        // self.nodes.read().unwrap().get(&id).map(|node| node.clone())
        todo!()
    }

    pub(crate) fn remove_node(&self, node_id: NodeId) -> Option<Address<NodeActor>> {
        self.nodes.write().unwrap().remove(&node_id)
    }
}

//------------------------------------------------------------------------------------------------
//  FindProcess
//------------------------------------------------------------------------------------------------

pub struct FindProcessRequest<A: Actor> {
    receiver: oneshot::Receiver<Option<Pid<A>>>
}

impl<A: Actor> FindProcessRequest<A> {
    pub fn new() -> (oneshot::Sender<Option<Pid<A>>>, Self) {
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
    Io(std::io::Error),
}

impl From<std::io::Error> for AddNodeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
