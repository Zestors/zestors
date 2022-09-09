use crate::{ConnectError, SystemRef};
use bytes::{Buf, Bytes, BytesMut};
use futures::{io, SinkExt, StreamExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_util::codec::{Framed, FramedWrite, LengthDelimitedCodec};
use zestors::{
    config::Config,
    error::RecvError,
    process::{spawn, Address, Child, Inbox},
};
use zestors_codegen::protocol;
use zestors_core as zestors;

pub(crate) type NodeId = u64;

#[derive(Debug, Clone)]
pub struct NodeRef {
    address: Address<NodeMsg>,
    shared: Arc<NodeShared>,
}

impl NodeRef {
    pub(crate) async fn spawn_connect(
        socket: SocketAddr,
        system: SystemRef,
    ) -> Result<(Child<NodeExit>, Self), ConnectError> {
        match TcpStream::connect(socket).await {
            Ok(stream) => Self::spawn(stream, socket, system).await,
            Err(e) => Err(ConnectError::TcpSetupFailed(e)),
        }
    }

    pub(crate) async fn spawn(
        mut stream: TcpStream,
        socket: SocketAddr,
        system: SystemRef,
    ) -> Result<(Child<NodeExit>, Self), ConnectError> {
        // Exchange id's with the other
        stream.write_u64(system.id()).await?;
        let node_id = stream.read_u64().await?;

        let shared = Arc::new(NodeShared::new(socket.clone(), node_id, system));
        let shared_clone = shared.clone();

        let (child, address) = spawn(Config::default(), |inbox| async move {
            Node::new(shared_clone, stream, inbox).run().await
        });

        Ok((child.into_dyn(), NodeRef { address, shared }))
    }

    pub fn id(&self) -> NodeId {
        self.shared.id()
    }
}

#[derive(Debug)]
pub(super) struct Node {
    shared: Arc<NodeShared>,
    stream: MessageStream,
    inbox: Inbox<NodeMsg>,
}

#[derive(Debug)]
struct MessageStream {
    stream: Framed<TcpStream, LengthDelimitedCodec>,
}

impl MessageStream {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream: Framed::new(stream, LengthDelimitedCodec::new()),
        }
    }

    pub async fn send(&mut self, msg: &BinaryMessage) -> Result<(), io::Error> {
        let msg = Bytes::from(bincode::serialize(msg).unwrap());
        self.stream.send(msg).await
    }

    pub async fn recv(&mut self) -> Result<BinaryMessage, io::Error> {
        match self.stream.next().await {
            Some(msg) => match msg {
                Ok(msg) => Ok(bincode::deserialize_from(msg.reader()).unwrap()),
                Err(e) => Err(e),
            },
            None => todo!(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum BinaryMessage {
    Ping,
    Pong,
}

impl Node {
    pub fn new(shared: Arc<NodeShared>, stream: TcpStream, inbox: Inbox<NodeMsg>) -> Self {
        Self {
            shared,
            stream: MessageStream::new(stream),
            inbox,
        }
    }

    pub async fn run(mut self) -> NodeExit {
        loop {
            tokio::select! {
                res = self.stream.recv() => {
                    if let Err(exit) = self.handle_tcp(res).await {
                        break exit;
                    }
                }

                res = self.inbox.recv() => {
                    if let Err(exit) = self.handle_inbox(res).await {
                        break exit;
                    }
                }
            }
        }
    }

    pub async fn handle_tcp(
        &mut self,
        res: Result<BinaryMessage, io::Error>,
    ) -> Result<(), NodeExit> {
        match res {
            Ok(msg) => match msg {
                BinaryMessage::Ping => self.stream.send(&BinaryMessage::Pong).await.unwrap(),
                BinaryMessage::Pong => todo!(),
            },
            Err(e) => todo!(),
        };
        Ok(())
    }

    pub async fn handle_inbox(&mut self, res: Result<NodeMsg, RecvError>) -> Result<(), NodeExit> {
        match res {
            Ok(_) => todo!(),
            Err(e) => match e {
                RecvError::Halted => {
                    self.inbox.close();
                    Ok(())
                }
                RecvError::ClosedAndEmpty => Err(NodeExit::Halted),
            },
        }
    }
}

#[derive(Debug)]
pub enum NodeExit {
    Halted,
}

#[protocol]
#[derive(Debug)]
pub(super) enum NodeMsg {}

#[derive(Debug)]
pub(super) struct NodeShared {
    socket: SocketAddr,
    system: SystemRef,
    id: u64,
}

impl NodeShared {
    pub fn new(socket: SocketAddr, id: NodeId, system: SystemRef) -> Self {
        Self { socket, id, system }
    }
}

impl NodeShared {
    pub fn id(&self) -> NodeId {
        self.id
    }
}
