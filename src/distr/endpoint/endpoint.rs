use crate::{core::*, distr::*, Action, Fn};
use futures::{
    future::{select_all, SelectAll},
    FutureExt,
};
use log::{info, warn};
use quinn::IdleTimeout;
use std::{
    io,
    net::{ToSocketAddrs, UdpSocket},
};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

//------------------------------------------------------------------------------------------------
//  Endpoint
//------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Endpoint {
    id: NodeId,
    endpoint_addr: EndpointAddr,
    node_store: NodeStore,
    quinn_endpoint: quinn::Endpoint,
}

impl Endpoint {
    pub fn socket_addr(&self) -> Result<std::net::SocketAddr, std::io::Error> {
        self.quinn_endpoint.local_addr()
    }

    pub(crate) fn addr(&self) -> &EndpointAddr {
        &self.endpoint_addr
    }

    pub(crate) fn node_store(&self) -> &NodeStore {
        &self.node_store
    }

    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Attempt to setup an insecure connection to another Endpoint located at the socket address.
    pub async fn connect_insecure<A>(&self, socket_addr: A) -> Result<Node, NodeConnectError>
    where
        A: std::net::ToSocketAddrs,
    {
        // Parse the socket address.
        let socket_addr = parse_socket_addr(socket_addr)?;

        // start connecting to the addresss
        let connecting = self.quinn_endpoint.connect(socket_addr, "insecure")?;

        // Spawn the node with this conneciton
        let system_node = NodeActor::spawn(connecting, self.clone()).await?;

        // and finally register the node on this system
        let node = self
            .endpoint_addr
            .register_new_outgoing_node(system_node)
            .into_rcv()
            .await??;

        Ok(node)
    }

    /// Create a new insecure Endpoint, given the socket address to bind on and the unique NodeID
    /// for this Endpoint.
    pub async fn spawn_insecure<A>(
        socket_addr: A,
        node_id: NodeId,
    ) -> Result<(Child<EndpointActor>, Self), NewEndpointError>
    where
        A: std::net::ToSocketAddrs,
    {
        // Create the address
        let socket_addr = parse_socket_addr(socket_addr)?;

        // Bind to the socket
        let socket = UdpSocket::bind(socket_addr)
            .map_err(|e| NewEndpointError::UdpSocketBindingFailed(e))?;

        // Create the configs
        let server_config = insecure_server_config();
        let client_config = insecure_client_config();
        let endpoint_config = endpoint_config();

        // Setup the QUIC endpoint
        let (mut endpoint, incoming) =
            quinn::Endpoint::new(endpoint_config, Some(server_config), socket)
                .map_err(|e| NewEndpointError::QUICSetupFailed(e))?;
        endpoint.set_default_client_config(client_config);

        let (child, endpoint) = EndpointActor::spawn(endpoint, incoming, node_id).await;

        info!(
            "[E{}] Created new insecure System on {}.",
            endpoint.id(),
            socket_addr
        );

        // And return the endpoint
        Ok((child, endpoint))
    }

    pub(crate) fn new(addr: EndpointAddr, endpoint: quinn::Endpoint, node_id: NodeId) -> Self {
        Self {
            endpoint_addr: addr,
            node_store: NodeStore::new(),
            quinn_endpoint: endpoint,
            id: node_id,
        }
    }
}

fn parse_socket_addr<A: ToSocketAddrs>(addr: A) -> Result<SocketAddr, ParseSocketAddrError> {
    addr.to_socket_addrs()
        .map_err(|_e| ParseSocketAddrError)?
        .next()
        .ok_or_else(|| ParseSocketAddrError)
}

#[derive(Debug, ThisError)]
#[error("Failed to parse socket addr")]
struct ParseSocketAddrError;

unsafe impl Send for Endpoint {}

/// An error returned when creating a new endpoint
#[derive(Debug, ThisError)]
#[error("Creation of new Endpoint failed:")]
pub enum NewEndpointError {
    /// Failed to bind to the socket.
    #[error("Couldn't to bind to socket. ({0})")]
    UdpSocketBindingFailed(std::io::Error),
    /// Setting up QUIC failed.
    #[error("Couldn't  to setup QUIC. ({0})")]
    QUICSetupFailed(std::io::Error),
    /// Socket address is invalid
    #[error("Couldn't  socket address")]
    InvalidSocketAddr,
}

#[derive(Debug, ThisError)]
#[error("Failed to connect to node: ")]
pub enum NodeConnectError {
    #[error("Couldn't register node. ({0})")]
    RegistrationFailure(#[from] NodeRegistrationError),
    #[error("Invalid socket address.")]
    InvalidSocketAddr,
    #[error("Endpoint has been closed.")]
    EndpointClosed,
    #[error("Could not connect. ({0})")]
    ConnectFailure(#[from] quinn::ConnectError),
    #[error("Connection error. ({0})")]
    ConnectionFailure(#[from] io::Error),
}

impl From<ParseSocketAddrError> for NodeConnectError {
    fn from(_: ParseSocketAddrError) -> Self {
        Self::InvalidSocketAddr
    }
}

impl From<ParseSocketAddrError> for NewEndpointError {
    fn from(_: ParseSocketAddrError) -> Self {
        Self::InvalidSocketAddr
    }
}

impl From<NodeSpawnError> for NodeConnectError {
    fn from(e: NodeSpawnError) -> Self {
        match e {
            NodeSpawnError::ConnectionFailure(e) => Self::ConnectionFailure(e),
        }
    }
}

impl From<SndRcvError<NodeChild>> for NodeConnectError {
    fn from(_e: SndRcvError<NodeChild>) -> Self {
        NodeConnectError::EndpointClosed
    }
}
