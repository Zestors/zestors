use std::any::Any;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_tungstenite::MaybeTlsStream;

use crate::distributed::node::Node;

use super::challenge;
use super::challenge::ChallengeError;
use super::cluster::AddNodeError;
use super::local_node::LocalNode;
use super::msg;
use super::ws_stream::WsRecvError;
use super::ws_stream::WsSendError;
use super::ws_stream::WsStream;
use super::NodeId;
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

    /// Spawn a new server that starts listening on the given port. It will spawn tokio::tasks
    /// for every new incoming connection, and add the nodes to the `LocalNode`.
    pub(crate) fn spawn(self) -> (async_channel::Sender<ServerMsg>, JoinHandle<ServerExit>) {
        let (sender, receiver) = async_channel::unbounded();

        let handle = tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    // Either we receive a new connection
                    res = self.listener.accept() => {
                        match res {
                            Ok((stream, addr)) => {
                                Node::spawn_as_server(self.local_node.clone(), stream, addr);
                            },
                            Err(e) => break ServerExit::Io(e),
                        }
                    }
                    // Or we receive a message
                    msg = receiver.recv() => {
                        match msg {
                            Ok(msg) => match msg {
                                ServerMsg::Stop => break ServerExit::StopMsgReceived
                            },
                            Err(_) => break ServerExit::AllAddressesDropped,
                        }
                    }
                }
            }
        });

        (sender, handle)
    }
}

#[derive(Debug)]
pub(crate) enum ServerExit {
    Ok,
    Io(std::io::Error),
    AllAddressesDropped,
    StopMsgReceived,
}

#[derive(Debug)]
pub(crate) enum ServerMsg {
    Stop,
}

#[derive(Debug)]
pub(crate) enum NodeExit {
    NodeDisconnectedNormal,
    NodeDisconnectAbrupt(tungstenite::Error),
}

#[derive(Debug)]
pub(crate) enum NodeMsg {}

#[derive(Debug)]
pub enum NodeConnectError {
    /// Could not set up a tcp-connection to the address.
    Io(std::io::Error),
    /// The websocket handshake failed.
    HandShakeFailed(tungstenite::Error),
    /// The protocols did not match up.
    Protocol(Box<dyn Any + Send>, &'static str),
    /// The challenge failed.
    ChallengeFailed(ChallengeError),
    /// There was a problem receiving a message through the ws.
    WsRecv(Box<dyn std::fmt::Debug + Send>),
    /// There was a problem sending a message through the ws.
    WsSend(WsSendError),

    NodeIdAlreadyRegistered,

    NodeIdIsLocalNode,
}

impl From<WsRecvError> for NodeConnectError {
    fn from(e: WsRecvError) -> Self {
        Self::WsRecv(Box::new(e))
    }
}

impl From<WsSendError> for NodeConnectError {
    fn from(e: WsSendError) -> Self {
        Self::WsSend(e)
    }
}

impl From<ChallengeError> for NodeConnectError {
    fn from(e: ChallengeError) -> Self {
        Self::ChallengeFailed(e)
    }
}

impl From<std::io::Error> for NodeConnectError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<AddNodeError> for NodeConnectError {
    fn from(e: AddNodeError) -> Self {
        match e {
            AddNodeError::NodeIdAlreadyRegistered => Self::NodeIdAlreadyRegistered,
            AddNodeError::NodeIdIsLocalNode => Self::NodeIdIsLocalNode
        }
    }
}
