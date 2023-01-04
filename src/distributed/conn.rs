use std::net::SocketAddr;

use crate::{self as zestors, *};
use tokio::net::{TcpListener, TcpStream};
use zestors_codegen::Message;

//------------------------------------------------------------------------------------------------
//  Public
//------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Conn {
    address: Address<ConnProtocol>,
}

#[derive(Debug)]
pub struct ConnExit;

impl Conn {
    pub(crate) fn spawn_incoming(stream: TcpStream) -> (Child<ConnExit, ConnProtocol>, Conn) {
        let (child, address) = spawn_process(Config::default(), move |inbox| async move {
            ConnActor::new(inbox, stream).run().await
        });

        (child, Conn { address })
    }

    pub(crate) fn spawn_outgoing(
        peer_sock: SocketAddr,
        tx: Tx<Result<Conn, ConnectError>>,
    ) -> (Child<ConnExit, ConnProtocol>, Conn) {
        let (conn_tx, conn_rx) = new::<Conn>();

        let (child, address) = spawn_process(Config::default(), move |inbox| async move {
            if let Ok(stream) = TcpStream::connect(peer_sock).await {
                let conn = conn_rx.await.unwrap();
                let _ = tx.send(Ok(conn));
                ConnActor::new(inbox, stream).run().await
            } else {
                let _ = tx.send(Err(ConnectError::Todo));
                ConnExit
            }
        });

        let conn = Conn { address };
        let _ = conn_tx.send(conn.clone());

        (child, conn)
    }
}

//------------------------------------------------------------------------------------------------
//  Private
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) struct ConnActor {
    inbox: Inbox<ConnProtocol>,
    stream: TcpStream,
}

impl ConnActor {
    fn new(inbox: Inbox<ConnProtocol>, stream: TcpStream) -> Self {
        Self { inbox, stream }
    }
    async fn run(self) -> ConnExit {
        ConnExit
    }
}

#[protocol]
#[derive(Debug)]
pub(crate) enum ConnProtocol {}
