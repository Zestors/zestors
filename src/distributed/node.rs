use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    pin::Pin,
};

use futures::{Future, Stream, StreamExt};
use indexmap::{IndexMap, IndexSet};
use tokio::net::TcpStream;
use tokio_tungstenite::MaybeTlsStream;

use crate::{
    Fn,
    action::{Action, AsyncMsgFnType},
    actor::{Actor, ExitReason},
    address::{Address, Addressable},
    context::{BasicCtx, NoCtx, StreamItem},
    distributed::{
        msg::{ActorTypeId, CheckForProcessFn},
        registry::Registry,
    },
    flows::{InitFlow, MsgFlow, ReqFlow},
    messaging::Request,
};

use super::{
    challenge::{self, VerifiedStream},
    cluster::AddNodeError,
    local_node::LocalNode,
    msg::{self, Id, NextId},
    pid::{AnyPid, Pid},
    registry::RegistryGetError,
    remote_action::RemoteAction,
    ws_stream::{WsRecvError, WsStream},
    NodeId, ProcessId,
};

#[derive(Clone, Debug)]
pub(crate) struct Node {
    address: Address<NodeActor>,
}

//------------------------------------------------------------------------------------------------
//  NodeActor
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) struct NodeActor {
    /// Reference to the local node
    local_node: LocalNode,
    /// The id of this node
    node_id: NodeId,
    /// The stream, which can be used to communicate with this node
    ws: VerifiedStream,
    /// Outstanding find_process requests for processes on this node
    outstanding_find_processes: HashMap<Id, Request<Result<(), RegistryGetError>>>,
    /// Outstanding messages for processes on this node
    outstanding_messages: HashMap<Id, Request<()>>,
    /// The id that will be used for sending the next message
    next_id: NextId,

    address: Address<Self>,
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

impl Actor for NodeActor {
    type Init = (LocalNode, TcpStream);
    type Ctx = Self;
    type InitError = AddNodeError;

    fn init(
        (local_node, stream): Self::Init,
        address: Address<Self>,
    ) -> Pin<Box<dyn Future<Output = InitFlow<Self>> + Send>> {
        Box::pin(async {
            let stream = WsStream::handshake_as_server(MaybeTlsStream::Plain(stream))
                .await
                .unwrap();

            // Do challenge as the server
            let (node_id, ws) = match challenge::as_server(&local_node, stream).await {
                Ok(stream) => stream,
                Err(_e) => return InitFlow::Error(todo!()),
            };

            let addr = ws.peer_addr();

            // // If the node to be added has the same id as this node, reject
            if local_node.node_id() == node_id {
                return InitFlow::Error(AddNodeError::NodeIdIsLocalNode);
            }

            InitFlow::Init(
                Self {
                    node_id,
                    local_node,
                    ws,
                    outstanding_find_processes: HashMap::new(),
                    outstanding_messages: HashMap::new(),
                    next_id: NextId::new(),
                    address,
                }
            )
        })
    }

    fn exit(self, reason: ExitReason<Self>) -> Self::ExitWith {
        // self.local_node.cluster().remove_node(self.node_id).unwrap();
        reason
    }

    fn ctx(&mut self) -> &mut Self::Ctx {
        self
    }
}

//------------------------------------------------------------------------------------------------
//  Impl
//------------------------------------------------------------------------------------------------

impl NodeActor {
    fn spawn_as_server(local_node: LocalNode, stream: TcpStream) {}

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
