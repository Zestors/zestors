use futures::SinkExt;
use futures::StreamExt;
use hyper::Request;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::distributed::challenge::Challenge;
use crate::distributed::challenge::ChallengeResponse;
use crate::distributed::protocol::Protocol;
use crate::distributed::protocol::ProtocolStream;

use super::challenge::ChallengeError;
use super::local_node;
use super::local_node::LocalNode;
use super::local_node::NodeAlreadyConnected;
use super::protocol::DeserializationError;
use super::protocol::ProtocolRecv;
use super::protocol::ProtocolSendError;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite;

//--------------------------------------------------------------------------------------------------
//  Server
//--------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) struct Server {
    listener: TcpListener,
    local_node: LocalNode,
}

impl Server {
    pub(crate) fn new(listener: TcpListener, local_node: LocalNode) -> Self {
        Self {
            listener,
            local_node,
        }
    }

    pub(crate) fn spawn(self) -> (async_channel::Sender<ServerMsg>, JoinHandle<ServerExit>) {
        let (sender, receiver) = async_channel::unbounded();

        let handle = tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    // Either we receive a new connection
                    res = self.listener.accept() => {
                        match res {
                            Ok((stream, addr)) => {
                                Self::incoming_node(self.local_node.clone(), stream, addr)
                            },
                            Err(_e) => (),
                        }

                    }
                    // Or we receive a message
                    msg = receiver.recv() => {
                        match msg {
                            Ok(msg) => match msg {
                                ServerMsg::Stop => break ServerExit::StopMsgReceived
                            },
                            Err(e) => break ServerExit::AllAddressesDropped,
                        }
                    }
                }
            }
        });

        (sender, handle)
    }

    pub(crate) fn incoming_node(local_node: LocalNode, stream: TcpStream, addr: SocketAddr) {
        tokio::task::spawn(async move {
            match Node::incoming_setup_and_challenge(&local_node, stream, addr).await {
                Ok((node_id, stream)) => match local_node.spawn_node(node_id, stream) {
                    Ok(_) => (),
                    Err(e) => println!("Already registered: {:?}", e),
                },
                Err(e) => println!("challenge failed: {:?}", e),
            }
        });
    }
}

#[derive(Debug)]
pub(crate) enum ServerExit {
    Ok,
    AllAddressesDropped,
    StopMsgReceived,
}

#[derive(Debug)]
pub(crate) enum ServerMsg {
    Stop,
}

//--------------------------------------------------------------------------------------------------
//  Node
//--------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct Node(Arc<InnerNodeConn>);

#[derive(Debug)]
pub(crate) struct InnerNodeConn {
    node_id: Uuid,
    handle: JoinHandle<NodeExit>,
    addr: SocketAddr,
    sender: async_channel::Sender<NodeMsg>,
}

impl Node {
    pub fn node_id(&self) -> &Uuid {
        &self.0.node_id
    }

    pub(crate) fn new(
        node_id: Uuid,
        handle: JoinHandle<NodeExit>,
        sender: async_channel::Sender<NodeMsg>,
        addr: SocketAddr,
    ) -> Self {
        Self(Arc::new(InnerNodeConn {
            node_id,
            handle,
            sender,
            addr,
        }))
    }

    pub(crate) async fn spawned(
        local_node: LocalNode,
        node_id: Uuid,
        stream: ProtocolStream,
        receiver: async_channel::Receiver<NodeMsg>,
    ) -> NodeExit {
        println!("Node has been spawned!");
        NodeExit::NodeDisconnectedNormal
    }

    async fn incoming_setup_and_challenge(
        local_node: &LocalNode,
        stream: TcpStream,
        addr: SocketAddr,
    ) -> Result<(Uuid, ProtocolStream), NodeConnectingError> {
        let mut stream = ProtocolStream::new_server_no_tls(stream).await?;

        //----------------------------------------------------------------------------------------------
        //  Challenging the client
        //----------------------------------------------------------------------------------------------
        let challenge = Challenge::new();
        stream.send(Protocol::Challenge(challenge)).await?;

        match stream.recv_next().await {
            ProtocolRecv::Protocol(Protocol::ChallengeReply(challenge_response)) => {
                match challenge_response.check_challenge(&challenge, &local_node) {
                    Err(e) => {
                        stream
                            .send(Protocol::ChallengeResult(Err(e.clone())))
                            .await?;
                        return Err(e.into());
                    }
                    Ok(_) => {
                        stream.send(Protocol::ChallengeResult(Ok(()))).await?;
                    }
                }
            }
            val => return Err(NodeConnectingError::DidntAdhereToProtocol(Box::new(val))),
        };

        //----------------------------------------------------------------------------------------------
        //  Challenged by client
        //----------------------------------------------------------------------------------------------

        // wait for challenge
        match stream.recv_next().await {
            ProtocolRecv::Protocol(Protocol::Challenge(challenge)) => {
                let response = ChallengeResponse::new(&local_node, &challenge);
                stream.send(Protocol::ChallengeReply(response)).await?;
            }
            val => return Err(NodeConnectingError::DidntAdhereToProtocol(Box::new(val))),
        }

        // wait for challenge result
        match stream.recv_next().await {
            ProtocolRecv::Protocol(Protocol::ChallengeResult(result)) => {
                if let Err(e) = result {
                    return Err(e.into());
                };
            }
            val => return Err(NodeConnectingError::DidntAdhereToProtocol(Box::new(val))),
        }

        //----------------------------------------------------------------------------------------------
        //  Exchanging node ids
        //----------------------------------------------------------------------------------------------

        stream
            .send(Protocol::SendNodeId(local_node.node_id()))
            .await?;
        let peer_node_id = match stream.recv_next().await {
            ProtocolRecv::Protocol(Protocol::SendNodeId(id)) => id,
            val => return Err(NodeConnectingError::DidntAdhereToProtocol(Box::new(val))),
        };

        Ok((peer_node_id, stream))
    }

    pub(crate) async fn outgoing_setup_and_challenge(
        local_node: &LocalNode,
        addr: SocketAddr,
    ) -> Result<(Uuid, ProtocolStream), NodeConnectingError> {
        let mut stream = ProtocolStream::new_client_no_tls(addr).await?;

        //----------------------------------------------------------------------------------------------
        //  Challenged by server
        //----------------------------------------------------------------------------------------------

        // wait for challenge
        match stream.recv_next().await {
            ProtocolRecv::Protocol(Protocol::Challenge(challenge)) => {
                let response = ChallengeResponse::new(&local_node, &challenge);
                stream.send(Protocol::ChallengeReply(response)).await?;
            }
            val => return Err(NodeConnectingError::DidntAdhereToProtocol(Box::new(val))),
        }

        // wait for challenge result
        match stream.recv_next().await {
            ProtocolRecv::Protocol(Protocol::ChallengeResult(result)) => {
                if let Err(e) = result {
                    return Err(e.into());
                };
            }
            val => return Err(NodeConnectingError::DidntAdhereToProtocol(Box::new(val))),
        }

        //----------------------------------------------------------------------------------------------
        //  Challenging the server
        //----------------------------------------------------------------------------------------------

        let challenge = Challenge::new();
        stream.send(Protocol::Challenge(challenge)).await?;

        match stream.recv_next().await {
            ProtocolRecv::Protocol(Protocol::ChallengeReply(challenge_response)) => {
                match challenge_response.check_challenge(&challenge, &local_node) {
                    Err(e) => {
                        stream
                            .send(Protocol::ChallengeResult(Err(e.clone())))
                            .await?;
                        return Err(e.into());
                    }
                    Ok(_) => {
                        stream.send(Protocol::ChallengeResult(Ok(()))).await?;
                    }
                }
            }
            val => return Err(NodeConnectingError::DidntAdhereToProtocol(Box::new(val))),
        };

        //----------------------------------------------------------------------------------------------
        //  Exchanging node ids
        //----------------------------------------------------------------------------------------------

        let peer_node_id = match stream.recv_next().await {
            ProtocolRecv::Protocol(Protocol::SendNodeId(id)) => id,
            val => return Err(NodeConnectingError::DidntAdhereToProtocol(Box::new(val))),
        };
        stream
            .send(Protocol::SendNodeId(local_node.node_id()))
            .await?;

        Ok((peer_node_id, stream))
    }
}

#[derive(Debug)]
pub(crate) enum NodeExit {
    NodeDisconnectedNormal,
    NodeDisconnectAbrupt(tungstenite::Error),
}

#[derive(Debug)]
pub(crate) enum NodeMsg {}

#[derive(Debug)]
pub enum NodeConnectingError {
    CantConnectToAddr(std::io::Error),
    HandShakeFailed(tungstenite::Error),
    DidntAdhereToProtocol(Box<dyn std::fmt::Debug + Send>),
    ChallengeFailed(ChallengeError),
    PeerDiedBeforeConn(tungstenite::Error),
    NodeAlreadyConnected,
}

impl From<ProtocolSendError> for NodeConnectingError {
    fn from(e: ProtocolSendError) -> Self {
        match e {
            ProtocolSendError::Any(e) => Self::PeerDiedBeforeConn(e),
        }
    }
}

impl From<ChallengeError> for NodeConnectingError {
    fn from(e: ChallengeError) -> Self {
        Self::ChallengeFailed(e)
    }
}

impl From<NodeAlreadyConnected> for NodeConnectingError {
    fn from(e: NodeAlreadyConnected) -> Self {
        Self::NodeAlreadyConnected
    }
}
