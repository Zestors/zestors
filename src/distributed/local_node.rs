use std::{
    any::Any,
    collections::HashMap,
    lazy::SyncOnceCell,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, RwLock},
};

use crate::{
    actor::{Actor, Spawn},
    address::Address,
    child::Child,
};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_native_tls::{
    native_tls::{self, TlsAcceptorBuilder},
    TlsAcceptor,
};
use tokio_tungstenite::{tungstenite, MaybeTlsStream};
use uuid::Uuid;

use super::{
    challenge,
    cluster::Cluster,
    node::Node,
    server::{NodeExit, NodeSetupError, Server, ServerExit, ServerMsg},
    ws_stream::WsStream,
    BuildId, Token, NodeId,
};

//--------------------------------------------------------------------------------------------------
//  LOCAL_NODE
//--------------------------------------------------------------------------------------------------

static LOCAL_NODE: SyncOnceCell<LocalNode> = SyncOnceCell::new();

/// Initialize the local node. Panics if already [initialize]d.
pub async fn initialize(
    token: Token,
    node_id: NodeId,
    socket: SocketAddr,
) -> Result<&'static LocalNode, InitializationError> {
    let local_node = LocalNode::initialize(token, node_id, socket).await?;
    LOCAL_NODE.set(local_node).unwrap();
    Ok(LOCAL_NODE.get().unwrap())
}

/// Get a static reference to the local node. Panics if node is not yet [initialize]d.
pub fn get() -> &'static LocalNode {
    LOCAL_NODE.get().unwrap()
}

//--------------------------------------------------------------------------------------------------
//  LocalNode
//--------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LocalNode(Arc<LocalNodeCore>);

impl LocalNode {
    pub async fn initialize(
        token: Token,
        node_id: NodeId,
        addr: SocketAddr,
    ) -> Result<Self, InitializationError> {
        // Start listening on the socket.
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| InitializationError::SocketAddressNotAvailable(e))?;

        // Create the local node.
        let local_node = Self::new(token, node_id, addr);

        // Spawn the task that will be listening
        let (sender, handle) = Server::new(listener, local_node.clone()).spawn();

        // set the `JoinHandle`.
        local_node.0.set_server_handle(sender, handle);

        Ok(local_node)
    }

    pub fn get_node(&self, node_id: NodeId) -> Option<Node> {
        self.0.cluster.get_node(node_id)
    }

    pub fn build_id(&self) -> BuildId {
        self.0.build_id
    }

    pub fn token(&self) -> Token {
        self.0.token
    }

    pub fn node_id(&self) -> NodeId {
        self.0.node_id
    }

    /// Attempt to connect directly to another node. This first challenges, and if successful
    /// creates a connection to this node.
    pub async fn connect(&self, addr: SocketAddr) -> Result<Node, NodeSetupError> {
        // Setup tcp stream
        let stream = TcpStream::connect(addr).await?;

        // upgrade to ws
        let stream = WsStream::handshake_as_client(MaybeTlsStream::Plain(stream)).await?;

        // challenge as client
        let (node_id, stream) = challenge::as_client(self, stream).await?;

        let node = self.0.cluster.spawn_node(self, node_id, stream)?;

        Ok(node)
    }

    pub(crate) fn get_cluster(&self) -> &Cluster {
        &self.0.cluster
    }


    fn new(token: Token, node_id: NodeId, addr: SocketAddr) -> Self {
        Self(Arc::new(LocalNodeCore {
            token,
            addr,
            // Get build_id from binary.
            build_id: build_id::get(),
            node_id,
            // No connected nodes.
            cluster: Cluster::new(),
            // No `Process<Server>` yet.
            server: SyncOnceCell::new(),
        }))
    }
}

//------------------------------------------------------------------------------------------------
//  Core
//------------------------------------------------------------------------------------------------

/// This is the local node, which can connect to other nodes that run the same binary. A node can be
/// initialized with a unique name (`String`), an addr
#[derive(Debug)]
pub(crate) struct LocalNodeCore {
    /// The token used to verify that a Node has permission to connect to the cluster.
    token: Uuid,
    /// The address that this node is listening on.
    addr: SocketAddr,
    /// The build_id used to verify that two Nodes have the same binary.
    build_id: BuildId,
    /// The name of this node, which can be used to find it from another node.
    node_id: NodeId,

    cluster: Cluster,
    // The  join_handle of the server task
    server: SyncOnceCell<(async_channel::Sender<ServerMsg>, JoinHandle<ServerExit>)>,
}

impl LocalNodeCore {
    /// Panics if already set!!
    fn set_server_handle(
        &self,
        sender: async_channel::Sender<ServerMsg>,
        process: JoinHandle<ServerExit>,
    ) {
        self.server.set((sender, process)).unwrap();
    }

    /// Panics if not yet set!
    fn server_handle(&self) -> &JoinHandle<ServerExit> {
        &self.server.get().unwrap().1
    }

    /// Panics if not yet set!
    fn server_sender(&self) -> &async_channel::Sender<ServerMsg> {
        &self.server.get().unwrap().0
    }
}

//--------------------------------------------------------------------------------------------------
//  RegisteredActor
//--------------------------------------------------------------------------------------------------

#[derive(Debug)]
enum RegisteredActor {
    Process(Box<dyn Any + Send + Sync>),
    Address(Box<dyn Any + Send + Sync>),
}

impl RegisteredActor {
    fn new_address<A: Actor>(address: Address<A>) -> Self {
        Self::Address(Box::new(address))
    }

    fn new_process<A: Actor>(process: Child<A>) -> Self {
        Self::Process(Box::new(process))
    }
}

unsafe impl<A: Actor> Sync for Child<A> {}

//--------------------------------------------------------------------------------------------------
//  Errors
//--------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub enum RegistrationError {
    AlreadyRegistered,
}

#[derive(Debug)]
pub enum UnRegistrationError {
    NotRegistered,
}

#[derive(Debug)]
pub enum InitializationError {
    SocketAddressNotAvailable(std::io::Error),
}
