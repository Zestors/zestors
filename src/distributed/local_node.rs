use std::{
    any::Any,
    collections::HashMap,
    convert::Infallible,
    lazy::SyncOnceCell,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, RwLock},
};

use crate::{
    actor::{Actor, Spawn},
    process::Process,
};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_tungstenite::tungstenite;
use uuid::Uuid;

use super::{
    protocol::ProtocolStream,
    server::{InnerNodeConn, Node, NodeConnectingError, NodeExit, Server, ServerExit, ServerMsg},
};

//--------------------------------------------------------------------------------------------------
//  LOCAL_NODE
//--------------------------------------------------------------------------------------------------

static LOCAL_NODE: SyncOnceCell<LocalNode> = SyncOnceCell::new();

/// Initialize the local node. Panics if already [initialize]d.
pub async fn initialize(
    cookie: Uuid,
    name: String,
    socket: SocketAddr,
) -> Result<&'static LocalNode, InitializationError> {
    let local_node = LocalNode::initialize(cookie, name, socket).await?;
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
pub struct LocalNode(Arc<InnerLocalNode>);

impl LocalNode {
    pub async fn initialize(
        cookie: Uuid,
        name: String,
        addr: SocketAddr,
    ) -> Result<Self, InitializationError> {
        // Start listening on the socket.
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| InitializationError::SocketAddressNotAvailable(e))?;

        // Create the local node.
        let local_node = Self::new(cookie, name, addr);

        // Spawn the task that will be listening
        let (sender, handle) = Server::new(listener, local_node.clone()).spawn();

        // set the `JoinHandle`.
        local_node.0.set_server_handle(sender, handle);

        Ok(local_node)
    }

    pub fn build_id(&self) -> Uuid {
        self.0.build_id
    }

    pub fn cookie(&self) -> Uuid {
        self.0.cookie
    }

    pub fn node_id(&self) -> Uuid {
        self.0.node_id
    }

    pub async fn connect(&self, addr: SocketAddr) -> Result<(), NodeConnectingError> {
        let (node_id, stream) = Node::outgoing_setup_and_challenge(self, addr).await?;
        self.spawn_node(node_id, stream)?;
        Ok(())
    }

    pub async fn connect_transitive(&self, addr: SocketAddr) -> Result<(), NodeConnectingError> {
        todo!()
    }

    /// spawn a node that has already been challenged
    pub(crate) fn spawn_node(
        &self,
        node_id: Uuid,
        stream: ProtocolStream,
    ) -> Result<Node, NodeConnectingError> {
        // aquire the lock
        let mut connected_nodes = self.0.connected_nodes.write().unwrap();
        if connected_nodes.contains_key(&node_id) {
            Err(NodeConnectingError::NodeAlreadyConnected)
        } else {
            let (sender, receiver) = async_channel::unbounded();
            let addr = stream
                .peer_addr()
                .map_err(|e| NodeConnectingError::PeerDiedBeforeConn(tungstenite::Error::Io(e)))?;

            let handle = tokio::task::spawn(Node::spawned(self.clone(), node_id, stream, receiver));

            let node = Node::new(node_id, handle, sender, addr);

            connected_nodes.insert(node_id, node.clone());

            Ok(node)
        }
    }

    fn new(cookie: Uuid, name: String, addr: SocketAddr) -> Self {
        Self(Arc::new(InnerLocalNode {
            cookie,
            addr,
            // Get build_id from binary.
            build_id: build_id::get(),
            name,
            // Generate a random node_id.
            node_id: Uuid::new_v4(),
            // No registered processes.
            registered: RwLock::new(HashMap::new()),
            // No connected nodes.
            connected_nodes: RwLock::new(HashMap::new()),
            // No `Process<Server>` yet.
            server: SyncOnceCell::new(),
        }))
    }
}

/// This is the local node, which can connect to other nodes that run the same binary. A node can be
/// initialized with a unique name (`String`), an addr
#[derive(Debug)]
pub(crate) struct InnerLocalNode {
    /// The cookie used to verify that a Node has permission to connect to the cluster.
    cookie: Uuid,
    /// The address that this node is listening on.
    addr: SocketAddr,
    /// The build_id used to verify that two Nodes have the same binary.
    build_id: Uuid,
    /// The name of this node, which can be used to find it from another node.
    name: String,
    /// The unique id of this node, randomly generated when this node spawns. Can be used to find
    /// this node from another node.
    node_id: Uuid,
    /// All processes that are currently registered on this node. They can be found from another
    /// node by their unique uid, generated upon registration.
    registered: RwLock<HashMap<Uuid, RegisteredActor>>,
    /// All nodes that are currently registered on this node.
    connected_nodes: RwLock<HashMap<Uuid, Node>>,
    // The  join_handle of the server task
    server: SyncOnceCell<(async_channel::Sender<ServerMsg>, JoinHandle<ServerExit>)>,
}

impl InnerLocalNode {
    pub fn register<A: Actor>(&self, process: &mut Process<A>) -> Result<(), RegistrationError> {
        match process.get_uuid() {
            Some(_) => Err(RegistrationError::AlreadyRegistered),
            None => {
                let uuid = Uuid::new_v4();

                self.registered.write().unwrap().insert(
                    uuid,
                    RegisteredActor::new_address::<A>(process.address().clone()),
                );

                process.set_uuid(uuid);

                Ok(())
            }
        }
    }

    pub fn unregister<A: Actor>(
        &self,
        process: &mut Process<A>,
    ) -> Result<Uuid, UnRegistrationError> {
        match process.unset_uuid() {
            Some(uuid) => {
                let removed = self.registered.write().unwrap().remove(&uuid);

                assert!(if let Some(RegisteredActor::Address(_)) = removed {
                    true
                } else {
                    false
                });
                Ok(uuid)
            }

            None => Err(UnRegistrationError::NotRegistered),
        }
    }

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
    fn new_address<A: Actor>(address: A::Address) -> Self {
        Self::Address(Box::new(address))
    }

    fn new_process<A: Actor>(process: Process<A>) -> Self {
        Self::Process(Box::new(process))
    }
}

unsafe impl<A: Actor> Sync for Process<A> {}

//--------------------------------------------------------------------------------------------------
//  Errors
//--------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct NodeAlreadyConnected(ProtocolStream);

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
