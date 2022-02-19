use std::{
    any::Any,
    net::SocketAddr,
    sync::{atomic::AtomicBool, Arc},
};

use async_channel::{Receiver, Sender};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    challenge::VerifiedStream,
    local_node::{self, LocalNode},
    NodeId, ws_message::WsMsg, ws_stream::WsRecvError, pid::Pid, ProcessId,
};

//------------------------------------------------------------------------------------------------
//  Node
//------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Node(Arc<ArcNode>);

impl Node {
    /// Spawn a thread with an open connection to the node. When spawning this node, it should also be
    /// registered on the local node.
    ///
    /// The node will automatically remove itself from the local node if it
    pub(crate) fn spawn(
        id: NodeId,
        addr: SocketAddr,
        mut stream: VerifiedStream,
        local_node: LocalNode,
    ) -> Self {
        let (sender, mut receiver) = async_channel::unbounded();
        let node = Self(Arc::new(ArcNode::new(id, addr, sender)));
        let cloned_node = node.clone();

        tokio::task::spawn(async move {
            // Run the node, until it disconnects
            node.running(&mut stream, &mut receiver, &local_node).await;

            // Node should always be unregistered after it is exiting.
            local_node.get_cluster()
                .remove_node(&node.0.id)
                .expect("Node should always be registered!");

            // Close the receiver, and drop all messages
            receiver.close();
            while let Ok(msg) = receiver.try_recv()  {
                drop(msg)
            }
        });

        cloned_node
    }

    async fn running(
        &self,
        stream: &mut VerifiedStream,
        receiver: &mut Receiver<NodeMsg>,
        local_node: &LocalNode,
    ) {
        loop {
            let msg = tokio::select! {
                ws_msg = stream.ws_stream().recv_next() => {
                    SelectMsg::Ws(ws_msg)
                }
                node_msg = receiver.next() => {
                    SelectMsg::Node(node_msg)
                }
            };

            match msg {
                SelectMsg::Node(Some(msg)) => todo!(),
                SelectMsg::Ws(Ok(msg)) => todo!(),
                SelectMsg::Node(None) => unreachable!("We own a copy of our own address"),
                SelectMsg::Ws(Err(e)) => match e {
                    WsRecvError::StreamClosed => break,
                    WsRecvError::Tungstenite(e) => {
                        panic!("recv_err: {:?}", e)
                    }
                    WsRecvError::NonBinaryMsg(_) => panic!("non-binary message!"),
                    WsRecvError::NotDeserializable(_) => panic!("not deserializable"),
                }
            }
        }

    }
}

//------------------------------------------------------------------------------------------------
//  InnerNode
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
struct ArcNode {
    id: NodeId,
    addr: SocketAddr,
    sender: Sender<NodeMsg>,
}

impl ArcNode {
    pub(crate) fn new(id: NodeId, addr: SocketAddr, sender: Sender<NodeMsg>) -> Self {
        Self {
            id,
            addr,
            sender,
        }
    }
}

//------------------------------------------------------------------------------------------------
//  NodeMsg
//------------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum NodeMsg {
    Message(ProcessId, Box<[u8]>),
    Disconnect,
}

enum SelectMsg {
    Node(Option<NodeMsg>),
    Ws(Result<WsMsg, WsRecvError>)
}