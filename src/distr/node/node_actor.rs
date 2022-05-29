use crate::{self as zestors, core::*, distr::*, Fn};
use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
use async_trait::async_trait;
use futures::{Future, FutureExt, SinkExt};
use log::{info, warn};
use quinn::{IdleTimeout, TransportConfig, VarInt};
use std::{io, pin::Pin, task::Poll, time::Duration};
use zestors_codegen::{zestors, Addr, NoScheduler};

/// A `Node` represents another `Endpoint`, which is connected to this `Endpoint`.
#[derive(Addr, Debug)]
#[addr(pub(crate) NodeAddr)]
pub(crate) struct NodeActor {
    connection: quinn::Connection,
    streams: quinn::IncomingBiStreams,
    node: Node,
    endpoint: Endpoint,
}

/// Implement the Actor trait for NodeActor
#[async_trait]
impl Actor for NodeActor {
    type Init = (quinn::Connecting, Endpoint);
    type Error = anyhow::Error;
    type Halt = ();
    type Exit = NodeActorExit;

    async fn initialize(
        (connecting, system): Self::Init,
        addr: Self::Addr<Local>,
    ) -> InitFlow<Self> {
        match connecting.await {
            Ok(new_conn) => match Self::initialize(new_conn, addr, system).await {
                Ok(actor) => InitFlow::Init(actor),
                Err(e) => InitFlow::Exit(NodeActorExit::InitFailed(e)),
            },
            Err(e) => {
                warn!("Failed to inialize new node: {}", e);
                InitFlow::Exit(NodeActorExit::InitFailed(e.into()))
            }
        }
    }

    fn handle_event(self, signal: Event<Self>, _state: &mut State<Self>) -> EventFlow<Self> {
        self.connection.close(VarInt::from_u32(0), &[]);
        info!(
            "[E{}] Node {} is exiting with Signal({:?})",
            self.endpoint.id(),
            self.node.id(),
            signal
        );
        EventFlow::Exit(NodeActorExit::Signal(signal))
    }
}

#[zestors(impl NodeAddr)]
impl NodeActor {
    /// After initialization, this will get called once in order to retrieve the `Node`.
    #[As(pub get_node)]
    fn handle_get_node(&mut self, _msg: (), snd: Snd<Node>) -> FlowResult<Self> {
        let _ = snd.send(self.node.clone());
        Ok(Flow::Cont)
    }
}

impl Stream for NodeActor {
    type Item = Action<Self>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.streams.poll_next_unpin(cx).map(|val| match val {
            Some(s) => match s {
                Ok(new_stream) => todo!("New stream accepted"),
                Err(e) => Some(Action::new(Fn!(Self::handle_connection_close), Some(e)).0),
            },
            None => Some(Action::new(Fn!(Self::handle_connection_close), None).0),
        })
    }
}

impl NodeActor {
    fn handle_connection_close(
        &mut self,
        error: Option<quinn::ConnectionError>,
    ) -> FlowResult<Self> {
        match error {
            Some(e) => warn!(
                "[E{}] {} - peer closed the connection: {}",
                self.endpoint.id(),
                self.node.id(),
                e
            ),
            None => warn!(
                "[E{}] {} - self closed the connection",
                self.endpoint.id(),
                self.node.id()
            ),
        }
        Ok(Flow::Halt(()))
    }

    /// Spawn a new node from a connection that has not yet been established.
    /// This method can be called both for incoming, as well as for outgoing connections.
    /// Afterwards, the node still has to be registered on the Endpoint.
    ///
    /// This will spawn the NodeActor, and then wait for it to be completely initialized.
    pub(crate) async fn spawn(
        connecting: quinn::Connecting,
        system: Endpoint,
    ) -> Result<NodeChild, NodeSpawnError> {
        // First spawn the new node
        let (child, addr): (Child<NodeActor>, NodeAddr) =
            spawn_actor::<NodeActor>((connecting, system.clone()));

        // Then attempt to retrieve the Node
        match addr.get_node(()).into_rcv().await {
            Ok(node) => {
                // Initialization was succesful, create the `NodeChild`.
                Ok(NodeChild::new(child, node))
            }
            Err(_) => {
                // Initialization was unsuccesful, now just get the reason why
                match child.await.unwrap() {
                    NodeActorExit::InitFailed(e) => Err(e),
                    _ => {
                        unreachable!("Should never fail with anything other that init failed")
                    }
                }
            }
        }
    }

    /// This function is called whenever the actor tries to initialize.
    async fn initialize(
        new_conn: quinn::NewConnection,
        peer_node_addr: NodeAddr,
        system: Endpoint,
    ) -> Result<Self, NodeSpawnError> {
        let quinn::NewConnection {
            connection,
            mut uni_streams,
            bi_streams,
            datagrams,
            ..
        } = new_conn;
        let peer_sock_addr = connection.remote_address();

        // First send own id
        let send_stream: AsyncBincodeWriter<_, &NodeId, _> = connection.open_uni().await?.into();
        let mut send_stream = send_stream.for_async();
        let this_system_id = system.id();
        send_stream.send(&this_system_id).await?;
        send_stream.into_inner().finish().await?;

        // Then attempt to receive an id
        let mut recv_stream: AsyncBincodeReader<_, NodeId> =
            uni_streams.next().await.unwrap()?.into();
        let peer_id = recv_stream.next().await.ok_or_else(|| {
            NodeSpawnError::ConnectionFailure(io::Error::new(
                io::ErrorKind::NotConnected,
                anyhow::anyhow!("Stream closed"),
            ))
        })??;

        Ok(Self {
            connection,
            streams: bi_streams,
            node: Node::new(peer_node_addr, peer_id, peer_sock_addr),
            endpoint: system,
        })
    }
}

//------------------------------------------------------------------------------------------------
//  NodeActorExit
//------------------------------------------------------------------------------------------------

/// When a NodeActor exits, this is the reason
#[derive(Debug)]
pub(crate) enum NodeActorExit {
    InitFailed(NodeSpawnError),
    Signal(Event<NodeActor>),
}

//------------------------------------------------------------------------------------------------
//  NodeSpawnError
//------------------------------------------------------------------------------------------------

/// An error when trying to spawn a new node
#[derive(Debug, ThisError)]
#[error("Failed to spawn node: ")]
pub enum NodeSpawnError {
    #[error("Couldn't connect. ({0})")]
    ConnectionFailure(#[from] io::Error),
}

impl From<quinn::ConnectionError> for NodeSpawnError {
    fn from(e: quinn::ConnectionError) -> Self {
        Self::ConnectionFailure(e.into())
    }
}

impl From<Box<bincode::ErrorKind>> for NodeSpawnError {
    fn from(e: Box<bincode::ErrorKind>) -> Self {
        match *e {
            bincode::ErrorKind::Io(io) => Self::ConnectionFailure(io),
            e => panic!("Invalid bincode encoding: {}", e),
        }
    }
}

impl From<quinn::WriteError> for NodeSpawnError {
    fn from(e: quinn::WriteError) -> Self {
        Self::ConnectionFailure(e.into())
    }
}
