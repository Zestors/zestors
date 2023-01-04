use std::net::SocketAddr;

use crate::{self as zestors, *};
use futures::io;
use tokio::net::TcpListener;

//------------------------------------------------------------------------------------------------
//  Public
//------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Node {
    address: Address<NodeProtocol>,
}

#[derive(Debug)]
pub struct NodeExit;

impl Node {
    pub async fn spawn(sock: SocketAddr) -> Result<(Child<NodeExit>, Node), io::Error> {
        let listener = TcpListener::bind(sock).await?;

        let (child, address) = spawn_process(Config::default(), move |inbox| async move {
            NodeActor::new(inbox, listener).run().await
        });

        Ok((child.into_dyn(), Node { address }))
    }

    pub async fn connect(&self, sock: SocketAddr) -> Result<Conn, ConnectError> {
        self.address.send(Connect(sock)).into_recv().await?
    }
}

//------------------------------------------------------------------------------------------------
//  Actor
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) struct NodeActor {
    inbox: Inbox<NodeProtocol>,
    listener: TcpListener,
    children: Vec<(Child<ConnExit, ConnProtocol>, Conn)>,
}

impl NodeActor {
    fn new(inbox: Inbox<NodeProtocol>, listener: TcpListener) -> Self {
        Self {
            inbox,
            listener,
            children: Vec::new(),
        }
    }

    async fn run(mut self) -> NodeExit {
        loop {
            tokio::select! {
                res = self.listener.accept() => {
                    let Ok((stream, _peer_sock)) = res else {
                        break NodeExit;
                    };
                    let (child, conn) = Conn::spawn_incoming(stream);
                    self.children.push((child, conn));
                }
                res = self.inbox.recv() => {
                    let Ok(msg) = res else {
                        break NodeExit;
                    };
                    match msg {
                        NodeProtocol::Connect((Connect(sock), tx)) => {
                            self.connect(sock, tx)
                        },
                    }
                }
            }
        }
    }

    fn connect(&mut self, sock: SocketAddr, tx: Tx<Result<Conn, ConnectError>>) {
        let (child, conn) = Conn::spawn_outgoing(sock, tx);
        self.children.push((child, conn));
    }
}

//------------------------------------------------------------------------------------------------
//  Protocol
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
#[protocol]
pub(crate) enum NodeProtocol {
    Connect(Connect),
}

#[derive(Message, Debug)]
#[msg(Rx<Result<Conn, ConnectError>>)]
pub(crate) struct Connect(SocketAddr);
