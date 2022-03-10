use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    pin::Pin,
};

use futures::{Future, Stream, StreamExt};
use indexmap::{IndexMap, IndexSet};
use tokio::net::TcpStream;
use tokio_tungstenite::MaybeTlsStream;

use crate::{
    action::{Action, AsyncMsgFnType},
    actor::{self, Actor, ExitReason, ProcessId},
    address::{Address, Addressable, Callable},
    child::{Child, ProcessExit},
    context::{BasicCtx, NoCtx, StreamItem},
    distributed::msg::{ActorTypeId, CheckForProcessFn},
    errors::DidntArrive,
    flows::{InitFlow, MsgFlow, ReqFlow},
    messaging::{Reply, Request},
    Fn,
};

use super::{
    challenge::{self, VerifiedStream},
    local_node::LocalNode,
    msg::{self, Id, NextId},
    pid::{AnyPid, ProcessRef},
    registry::RegistryGetError,
    remote_action::RemoteAction,
    server::NodeConnectError,
    ws_stream::{WsRecvError, WsStream},
    NodeId,
};

#[derive(Clone, Debug)]
pub struct Node {
    address: Address<NodeActor>,
    node_id: NodeId,
}

//------------------------------------------------------------------------------------------------
//  NodeActor
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) struct NodeActor {
    /// Reference to the local node
    local_node: LocalNode,
    /// The id of this node
    node: Node,
    /// The stream, which can be used to communicate with this node
    ws: VerifiedStream,
    /// Outstanding find_process requests for processes on this node
    outstanding_find_processes: HashMap<Id, Request<Result<(), RegistryGetError>>>,
    /// Outstanding messages for processes on this node
    outstanding_messages: HashMap<Id, Request<()>>,
    /// The id that will be used for sending the next message
    next_id: NextId,
}

//------------------------------------------------------------------------------------------------
//  Stream
//------------------------------------------------------------------------------------------------

impl Stream for NodeActor {
    type Item = StreamItem<NodeActor>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.ws.poll_next_unpin(cx).map(|val| {
            val.map(|val| StreamItem::Action(Action::new(Fn!(NodeActor::ws_message), val)))
        })
    }
}

//------------------------------------------------------------------------------------------------
//  Actor
//------------------------------------------------------------------------------------------------

pub enum NodeInit {
    Server(LocalNode, TcpStream, SocketAddr),
    Client(LocalNode, SocketAddr),
}

#[async_trait::async_trait]
impl Actor for NodeActor {
    type Init = NodeInit;
    type Ctx = Self;
    type InitError = NodeConnectError;

    async fn init(init: Self::Init, address: Address<Self>) -> InitFlow<Self> {
        let (local_node, ws, _addr, node_id) = match init {
            NodeInit::Server(local_node, stream, addr) => {
                let stream = WsStream::handshake_as_server(MaybeTlsStream::Plain(stream))
                    .await
                    .unwrap();

                let (node_id, ws) = challenge::as_server(&local_node, stream).await?;

                (local_node, ws, addr, node_id)
            }

            NodeInit::Client(local_node, addr) => {
                let stream = TcpStream::connect(addr).await?;

                let stream = WsStream::handshake_as_client(MaybeTlsStream::Plain(stream))
                    .await
                    .unwrap();

                let (node_id, ws) = challenge::as_client(&local_node, stream).await?;
                (local_node, ws, addr, node_id)
            }
        };

        let node = Node { address, node_id };

        local_node
            .cluster()
            .add_node(local_node.node_id(), node.clone())?;

        InitFlow::Init(Self {
            local_node,
            ws,
            outstanding_find_processes: HashMap::new(),
            outstanding_messages: HashMap::new(),
            next_id: NextId::new(),
            node,
        })
    }

    async fn exit(self, reason: ExitReason<Self>) -> Self::ExitWith {
        reason
    }

    fn ctx(&mut self) -> &mut Self::Ctx {
        self
    }

    fn debug() -> &'static str {
        "Node"
    }
}

//------------------------------------------------------------------------------------------------
//  Impl
//------------------------------------------------------------------------------------------------

impl Node {
    pub fn find_process<A: Actor>(
        &self,
        id: ProcessId,
    ) -> Result<Reply<Result<ProcessRef<A>, ()>>, ()> {
        todo!()
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn find_process_by_name<A: Actor>(
        &self,
        name: &'static str,
    ) -> Result<Reply<Result<ProcessRef<A>, ()>>, ()> {
        todo!()
    }

    pub(crate) fn send_action(
        &self,
        action: RemoteAction,
    ) -> Result<(), DidntArrive<RemoteAction>> {
        self.address.send(Fn!(NodeActor::send_action), action)
    }

    async fn get_node(address: &Address<NodeActor>) -> Result<Node, ()> {
        match address.send(Fn!(NodeActor::get_node), ()) {
            Ok(reply) => match reply.await {
                Ok(node) => Ok(node),
                Err(_) => Err(()),
            },
            Err(_) => Err(()),
        }
    }

    pub(crate) async fn spawn_as_client(
        local_node: LocalNode,
        addr: SocketAddr,
    ) -> Result<Self, NodeConnectError> {
        let mut child = actor::spawn::<NodeActor>(NodeInit::Client(local_node, addr));
        // todo: don't detatch, but instead handle this properly somewhere
        child.detach();

        match Self::get_node(child.address()).await {
            Ok(node) => Ok(node),
            Err(_) => match child.await {
                ProcessExit::InitFailed(e) => Err(e),
                ProcessExit::Handled(e) => {
                    panic!("{:?}", e)
                }
                ProcessExit::Panic(e) => panic!("{:?}", e),
                ProcessExit::HardAbort => panic!("hard abort"),
            },
        }
    }

    pub(crate) fn spawn_as_server(
        local_node: LocalNode,
        stream: TcpStream,
        addr: SocketAddr,
    ) -> Child<NodeActor> {
        let mut child = actor::spawn::<NodeActor>(NodeInit::Server(local_node, stream, addr));
        // todo: don't detatch, but instead handle this properly somewhere
        child.detach();
        child
    }
}

impl NodeActor {
    async fn ws_message(&mut self, msg_result: Result<msg::Msg, WsRecvError>) -> MsgFlow<Self> {
        match msg_result {
            Ok(msg) => match msg {
                msg::Msg::Challenge(_) => {
                    panic!("Shouldnt receive any more challenge messages now!")
                }
                msg::Msg::Process(msg) => match msg {
                    msg::CheckForProcess::Request(msg_id, process_id, check_fn) => {
                        let result =
                            unsafe { check_fn.call(self.local_node.registry(), process_id) };
                        self.ws
                            .send(msg::Msg::Process(msg::CheckForProcess::Reply(
                                msg_id, result,
                            )))
                            .await
                            .unwrap();
                    }
                    msg::CheckForProcess::Reply(msg_id, result) => {
                        let request = self.outstanding_find_processes.remove(&msg_id).unwrap();
                        let _ = request.reply(result);
                    }
                },
                msg::Msg::Action(_) => todo!(),
            },
            Err(_) => {}
        }

        MsgFlow::Ok
    }

    fn get_node(&mut self, _: ()) -> ReqFlow<Self, Node> {
        ReqFlow::Reply(self.node.clone())
    }

    pub(crate) async fn send_action(&mut self, action: RemoteAction) -> MsgFlow<Self> {
        let id = self.next_id.next();
        self.ws
            .send(msg::Msg::Action(msg::Action::Action(id, action)))
            .await
            .unwrap();

        MsgFlow::Ok
    }

    pub(crate) async fn find_process_by_id<A: Actor>(
        &mut self,
        id: ProcessId,
        request: Request<Result<(), RegistryGetError>>,
    ) -> MsgFlow<Self> {
        let msg_id = self.next_id.next();

        self.ws
            .send(msg::Msg::Process(msg::CheckForProcess::Request(
                msg_id,
                id,
                CheckForProcessFn::new::<A>(),
            )))
            .await?;

        let res = self.outstanding_find_processes.insert(msg_id, request);
        assert!(res.is_none());

        MsgFlow::Ok
    }

    pub(crate) fn test1(&mut self, action: RemoteAction) -> MsgFlow<Self> {
        MsgFlow::Ok
    }

    pub(crate) fn test2(&mut self, action: RemoteAction) -> ReqFlow<Self, ()> {
        ReqFlow::Reply(())
    }

    pub(crate) async fn test3(&mut self, action: RemoteAction) -> MsgFlow<Self> {
        MsgFlow::Ok
    }

    pub(crate) async fn test4(&mut self, action: RemoteAction) -> ReqFlow<Self, ()> {
        ReqFlow::Reply(())
    }

    pub(crate) fn test5(&mut self, action: RemoteAction, request: Request<()>) -> MsgFlow<Self> {
        MsgFlow::Ok
    }

    pub(crate) fn test6(&mut self, action: RemoteAction, request: Request<()>) -> MsgFlow<Self> {
        MsgFlow::Ok
    }
}
