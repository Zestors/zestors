use crate::{self as zestors, core::*, distr::*};
use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
use async_trait::async_trait;
use futures::{Future, FutureExt, SinkExt};
use log::warn;
use quinn::VarInt;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock}, net::SocketAddr,
};

pub type NodeId = u32;

/// A Node represents a connection to another Endpoint
#[derive(Debug, Clone)]
pub struct Node {
    node_addr: NodeAddr,
    id: NodeId,
    sock_addr: SocketAddr
}

impl Node {
    pub fn id(&self) -> NodeId {
        self.id
    }

    pub(crate) fn new(node_addr: NodeAddr, id: NodeId, sock_addr: SocketAddr) -> Self {
        Self {
            node_addr,
            id,
            sock_addr,
        }
    }
}

//------------------------------------------------------------------------------------------------
//  SystemNode
//------------------------------------------------------------------------------------------------

/// A NodeChild represents an uncloneable Node, paired with the Child of the NodeActor.
/// NodeChildren are stored within a NodeActor.
#[derive(Debug)]
pub(crate) struct NodeChild {
    node: Node,
    child: ActorChild<NodeActor>,
}
impl NodeChild {
    pub fn new(child: ActorChild<NodeActor>, node: Node) -> Self {
        Self { node, child }
    }

    pub fn node(&self) -> &Node {
        &self.node
    }

    pub fn id(&self) -> NodeId {
        self.node.id()
    }

    pub fn socket_addr(&self) -> SocketAddr {
        self.node.sock_addr
    }

    /// Stop the execution of this node, and wait for it to finish
    pub fn disconnect(&mut self) {
        self.child.soft_abort();
    }
}

impl Unpin for NodeChild {}

impl Future for NodeChild {
    type Output = Result<<NodeActor as Actor>::Exit, ExitError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.child.poll_unpin(cx)
    }
}



#[derive(Debug)]
pub(crate) struct NodeStoreInsertError(pub Node);
#[derive(Debug)]
pub(crate) struct NodeStoreRemoveError;
