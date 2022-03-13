use super::{
    challenge::ChallengeError,
    cluster::AddNodeError,
    local_node::{InitializationError, LocalNode},
    node::NodeActor,
    ws_stream::{WsRecvError, WsSendError},
    NodeId, Token,
};
use crate::{
    action::Action,
    actor::{self, Actor, ExitReason},
    address::{Address, Addressable},
    child::{Child, ProcessExit},
    context::StreamItem,
    distributed::node::Node,
    flows::{InitFlow, MsgFlow, ReqFlow}, function::{MsgFn, ReqFn},
};
use async_trait::async_trait;
use futures::{
    stream::{select_all, FuturesUnordered},
    FutureExt, Stream, StreamExt,
};
use std::{any::Any, net::SocketAddr, pin::Pin, task::Poll};
use tokio::{
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::tungstenite;

//--------------------------------------------------------------------------------------------------
//  Server
//--------------------------------------------------------------------------------------------------

/// This is the server, which communicates with the other nodes, and spawns tasks
#[derive(Debug)]
pub(crate) struct Server {
    listener: TcpListener,
    local_node: LocalNode,
    child_nodes: FuturesUnordered<Child<NodeActor>>,
}

impl Stream for Server {
    type Item = StreamItem<Self>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        match self.child_nodes.poll_next_unpin(cx) {
            Poll::Ready(Some(process_exit)) => Poll::Ready(Some(StreamItem::Action(Action::new(
                MsgFn::new_async(Self::child_node_exited),
                process_exit,
            )))),
            Poll::Pending | Poll::Ready(None) => match self.listener.poll_accept(cx) {
                Poll::Ready(res) => Poll::Ready(Some(StreamItem::Action(Action::new(
                    MsgFn::new_async(Self::incoming_connection),
                    res,
                )))),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

#[async_trait]
impl Actor for Server {
    type Init = (SocketAddr, Token, NodeId);
    type InitError = InitializationError;
    type Context = Self;

    async fn init((socket, token, node_id): Self::Init, address: Address<Self>) -> InitFlow<Self> {
        let listener = TcpListener::bind(socket).await?;

        let local_node = LocalNode::new(token, node_id, socket, address);

        InitFlow::Init(Self {
            listener,
            local_node,
            child_nodes: FuturesUnordered::new(),
        })
    }

    async fn exit(self, reason: ExitReason<Self>) -> Self::ExitWith {
        reason
    }

    fn ctx(&mut self) -> &mut Self::Context {
        self
    }
}

impl Server {
    pub(crate) async fn add_child(&mut self, child: Child<NodeActor>) -> MsgFlow<Self> {
        self.child_nodes.push(child);
        MsgFlow::Ok
    }

    async fn child_node_exited(&mut self, exit: ProcessExit<NodeActor>) -> MsgFlow<Self> {
        match exit {
            ProcessExit::Handled(exit) => {
                println!("Child node exited: {:?}", exit);
                MsgFlow::Ok
            }
            ProcessExit::InitFailed(exit) => {
                println!("Child node exited: {:?}", exit);
                MsgFlow::Ok
            }
            ProcessExit::Panic(_) => panic!(),
            ProcessExit::HardAbort => panic!(),
        }
    }

    async fn incoming_connection(
        &mut self,
        res: Result<(TcpStream, SocketAddr), std::io::Error>,
    ) -> MsgFlow<Self> {
        if let Ok((stream, addr)) = res {
            let child = Node::spawn_as_server(self.local_node.clone(), stream, addr);
            self.child_nodes.push(child)
        }

        MsgFlow::Ok
    }

    fn get_local_node(&mut self, _: ()) -> ReqFlow<Self, LocalNode> {
        ReqFlow::Reply(self.local_node.clone())
    }

    pub(crate) async fn spawn(
        socket: SocketAddr,
        token: Token,
        node_id: NodeId,
    ) -> Result<LocalNode, InitializationError> {
        let mut child = actor::spawn::<Server>((socket, token, node_id));
        child.detach();

        match child.addr().req(ReqFn::new_sync(Self::get_local_node), ()) {
            Ok(reply) => match reply.await {
                Ok(local_node) => Ok(local_node),
                Err(_) => Err(InitializationError::SocketAddressNotAvailable),
            },
            Err(_) => Err(InitializationError::SocketAddressNotAvailable),
        }
    }
}

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
            AddNodeError::NodeIdIsLocalNode => Self::NodeIdIsLocalNode,
        }
    }
}
