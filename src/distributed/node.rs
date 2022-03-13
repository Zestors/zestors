use std::{
    any::Any,
    borrow::Cow,
    collections::{HashMap, HashSet},
    net::SocketAddr,
    pin::Pin, marker::PhantomData,
};

use derive_more::DebugCustom;
use futures::{Future, FutureExt, Stream, StreamExt};
use indexmap::{IndexMap, IndexSet};
use tokio::net::TcpStream;
use tokio_tungstenite::MaybeTlsStream;

use crate::{
    action::{Action},
    actor::{self, Actor, ExitReason, ProcessId},
    address::{Address, Addressable},
    child::{Child, ProcessExit},
    context::{BasicCtx, NoCtx, StreamItem},
    distributed::msg::{ActorTypeId, FindProcessByIdFn, FindProcessByNameFn},
    errors::{DidntArrive, FindProcessError, ReqRecvError},
    flows::{InitFlow, MsgFlow, ReqFlow},
    function::{ReqFn},
    into_msg,
    messaging::{Reply, Request},
    Either,
};

use super::{
    challenge::{self, VerifiedStream},
    local_node::LocalNode,
    msg::{self, Id, NextId, RawMsg},
    pid::{AnyProcessRef, ProcessRef},
    registry::RegistryGetError,
    remote_action::{RemoteAction, RemoteHandlerReturn},
    server::NodeConnectError,
    ws_stream::{WsRecvError, WsStream},
    NodeId,
};

//------------------------------------------------------------------------------------------------
//  Node
//------------------------------------------------------------------------------------------------

#[derive(Clone, DebugCustom)]
#[debug(fmt = "Node({})", "node_id")]
pub struct Node {
    address: Address<NodeActor>,
    node_id: NodeId,
}

//------------------------------------------------------------------------------------------------
//  Node impl
//------------------------------------------------------------------------------------------------

unsafe impl Send for Node {}

impl Node {
    pub fn find_process_by_id<A: Actor>(
        &self,
        id: ProcessId,
    ) -> Result<Reply<Result<ProcessRef<A>, ()>>, ()> {
        todo!()
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn find_process_by_name_request<A: Actor>(
        &self,
        name: String,
    ) -> Result<ProcessRefReply<A>, DidntArrive<String>> {
        // self.address
        //     .call(Fn!(NodeActor::find_process_by_name::<A>), name)
        //     .map(|reply| ProcessRefReply(reply))
        todo!()
    }

    pub async fn find_process_by_name<A: Actor>(
        &self,
        name: String,
    ) -> Result<ProcessRef<A>, FindProcessError> {
        todo!()
        // match self
        //     .address
        //     .req_recv_old(Fn!(NodeActor::find_process_by_name::<A>), name)
        //     .await
        // {
        //     Ok(reply) => Ok(reply?),
        //     Err(e) => Err(FindProcessError::NodeDisconnected),
        // }
    }

    pub(crate) fn send_action(
        &self,
        action: RemoteAction,
    ) -> Result<(), DidntArrive<RemoteAction>> {
        // self.address.call(Fn!(NodeActor::send_action), action)
        todo!()
    }

    async fn get_node(address: &Address<NodeActor>) -> Result<Node, ReqRecvError<()>> {
        address
            .req_recv(ReqFn::new_sync(NodeActor::get_node), ())
            .await
    }

    pub(crate) async fn spawn_as_client(
        local_node: LocalNode,
        addr: SocketAddr,
    ) -> Result<(Self, Child<NodeActor>), NodeConnectError> {
        let child = actor::spawn::<NodeActor>(NodeInit::Client(local_node, addr));
        match Self::get_node(child.addr()).await {
            Ok(node) => Ok((node, child)),
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
        child
    }
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
    requests: HashMap<Id, NodeRequest>,
    /// The id that will be used for sending the next message
    next_id: NextId,
}

//------------------------------------------------------------------------------------------------
//  Stream for NodeActor
//------------------------------------------------------------------------------------------------

impl Stream for NodeActor {
    type Item = StreamItem<NodeActor>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // self.ws
        //     .poll_next_unpin(cx)
        //     .map(|val| val.map(|val| StreamItem::Action(Action::new(Fn!(NodeActor::ws_msg), val))))
        todo!()
    }
}

//------------------------------------------------------------------------------------------------
//  Actor for NodeActor
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub enum NodeInit {
    Server(LocalNode, TcpStream, SocketAddr),
    Client(LocalNode, SocketAddr),
}

#[derive(Debug)]
enum NodeRequest {
    FindProcess(ProcessRefRequest),
    Message(Request<()>),
}

#[derive(Debug)]
struct ProcessRefRequest {
    request: Box<dyn Any + Send>,
    reply_fn: usize,
}

#[derive(derive_more::Deref)]
pub struct ProcessRefReply<A>(Reply<Result<ProcessRef<A>, RegistryGetError>>);

impl<A> Future for ProcessRefReply<A> {
    type Output = Result<ProcessRef<A>, FindProcessError>;

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.0.poll_unpin(cx).map(|res| match res {
            Ok(res) => match res {
                Ok(reply) => Ok(reply),
                Err(e) => Err(e.into()),
            },
            Err(e) => Err(FindProcessError::NodeDisconnected),
        })
    }
}

impl ProcessRefRequest {
    pub fn new<A: Actor>(request: Request<Result<ProcessRef<A>, RegistryGetError>>) -> Self {
        Self {
            request: Box::new(request),
            reply_fn: Self::reply_fn::<A> as usize,
        }
    }

    pub fn reply(self, node: Node, id: ProcessId) {
        let reply_fn: fn(Self, Either<(Node, ProcessId), RegistryGetError>) =
            unsafe { std::mem::transmute(self.reply_fn) };
        reply_fn(self, Either::A((node, id)));
    }

    pub fn reply_err(self, err: RegistryGetError) {
        let reply_fn: fn(Self, Either<(Node, ProcessId), RegistryGetError>) =
            unsafe { std::mem::transmute(self.reply_fn) };
        reply_fn(self, Either::B(err));
    }

    fn reply_fn<A: Actor>(self, vals: Either<(Node, ProcessId), RegistryGetError>) {
        let request: Request<Result<ProcessRef<A>, RegistryGetError>> =
            *self.request.downcast().unwrap();

        match vals {
            Either::A((node, process_id)) => {
                let process_ref = unsafe { ProcessRef::<A>::new(node, process_id) };
                let _ = request.reply(Ok(process_ref));
            }
            Either::B(e) => {
                let _ = request.reply(Err(e));
            }
        }
    }
}

#[async_trait::async_trait]
impl Actor for NodeActor {
    type Init = NodeInit;
    type Context = Self;
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
            requests: HashMap::new(),
            next_id: NextId::new(),
            node,
        })
    }

    async fn exit(self, reason: ExitReason<Self>) -> Self::ExitWith {
        let res = self.local_node.cluster().remove_node(self.node.node_id());
        assert!(res.is_some());
        reason
    }

    fn ctx(&mut self) -> &mut Self::Context {
        self
    }

    fn debug() -> &'static str {
        "Node"
    }
}

//------------------------------------------------------------------------------------------------
//  Handling ws messages
//------------------------------------------------------------------------------------------------

impl NodeActor {
    async fn ws_msg(&mut self, msg: RawMsg) -> MsgFlow<Self> {
        match into_msg!(msg) {
            Ok(msg) => match msg {
                // RECEIVE: Challenge
                msg::Msg::Challenge(msg) => self.ws_challenge(msg).await,

                // RECEIVE: FindProcess
                msg::Msg::FindProcess(msg) => self.ws_find_process(msg).await,

                // RECEIVE: Action
                msg::Msg::Action(msg) => self.ws_action(msg).await,
            },

            // RECEIVE: Error
            Err(error) => self.ws_error(error).await,
        }
    }

    async fn ws_find_process<'a>(&mut self, msg: msg::FindProcess<'a>) -> MsgFlow<Self> {
        match msg {
            msg::FindProcess::RequestById(msg_id, process_id, lookup_fn) => {
                // Check whether the process is in the local registry,
                // safe since its the same binary
                let lookup_result =
                    unsafe { lookup_fn.call(self.local_node.registry(), process_id) };

                // Send back a reply using the same msg id
                self.ws
                    .send(msg::Msg::FindProcess(msg::FindProcess::Reply(
                        msg_id,
                        lookup_result,
                    )))
                    .await
                    .unwrap();
            }
            msg::FindProcess::RequestByName(msg_id, name, lookup_fn) => {
                // Check whether the process is in the local registry,
                // safe since its the same binary
                let lookup_result = unsafe { lookup_fn.call(self.local_node.registry(), &name) };

                // Send back a reply using the same msg id
                self.ws
                    .send(msg::Msg::FindProcess(msg::FindProcess::Reply(
                        msg_id,
                        lookup_result,
                    )))
                    .await
                    .unwrap();
            }
            msg::FindProcess::Reply(msg_id, result) => {
                if let NodeRequest::FindProcess(request) = self.requests.remove(&msg_id).unwrap() {
                    match result {
                        Ok(process_id) => {
                            let _ = request.reply(self.node.clone(), process_id);
                        }
                        Err(e) => {
                            let _ = request.reply_err(e);
                        }
                    }
                } else {
                    panic!()
                }
            }
        };

        MsgFlow::Ok
    }

    async fn ws_challenge(&mut self, msg: msg::Challenge) -> MsgFlow<Self> {
        MsgFlow::ExitWithError(anyhow::anyhow!(
            "Shouldnt receive challenge messages anymore"
        ))
    }

    async fn ws_action(&mut self, msg: msg::Action) -> MsgFlow<Self> {
        match msg {
            msg::Action::Action(msg_id, remote_action) => {
                match unsafe { remote_action.handle(self.local_node.registry()).await } {
                    RemoteHandlerReturn::Arrived => todo!(),
                    RemoteHandlerReturn::DidntArrive => todo!(),
                    RemoteHandlerReturn::ProcessNotFound => todo!(),
                }
            }
            msg::Action::Ack(msg_id, _) => todo!(),
            msg::Action::Reply(_, _) => todo!(),
        }

        MsgFlow::Ok
    }

    async fn ws_error(&mut self, error: WsRecvError) -> MsgFlow<Self> {
        MsgFlow::ExitWithError(error.into())
    }
}

//------------------------------------------------------------------------------------------------
//  Handling normal messages
//------------------------------------------------------------------------------------------------

impl NodeActor {
    fn get_node(&mut self, _: ()) -> ReqFlow<Self, Node> {
        ReqFlow::Reply(self.node.clone())
    }

    pub async fn send_action(&mut self, action: RemoteAction) -> MsgFlow<Self> {
        let id = self.next_id.next();
        self.ws
            .send(msg::Msg::Action(msg::Action::Action(id, action)))
            .await
            .unwrap();

        MsgFlow::Ok
    }

    async fn find_process_by_id<A: Actor>(
        &mut self,
        process_id: ProcessId,
        request: Request<Result<ProcessRef<A>, RegistryGetError>>,
    ) -> MsgFlow<Self> {
        let msg_id = self.next_id.next();

        self.ws
            .send(msg::Msg::FindProcess(msg::FindProcess::RequestById(
                msg_id,
                process_id,
                FindProcessByIdFn::new::<A>(),
            )))
            .await?;

        let res = self.requests.insert(
            msg_id,
            NodeRequest::FindProcess(ProcessRefRequest::new::<A>(request)),
        );
        assert!(res.is_none());

        MsgFlow::Ok
    }

    async fn find_process_by_name<A: Actor>(
        &mut self,
        name: String,
        request: Request<Result<ProcessRef<A>, RegistryGetError>>,
    ) -> MsgFlow<Self> {
        let msg_id = self.next_id.next();

        self.ws
            .send(msg::Msg::FindProcess(msg::FindProcess::RequestByName(
                msg_id,
                Cow::Owned(name),
                FindProcessByNameFn::new::<A>(),
            )))
            .await?;

        let res = self.requests.insert(
            msg_id,
            NodeRequest::FindProcess(ProcessRefRequest::new::<A>(request)),
        );
        assert!(res.is_none());

        MsgFlow::Ok
    }
}

