use std::{net::SocketAddr, task::Poll};

use futures::{SinkExt, Stream, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    tungstenite::{self, Message},
    MaybeTlsStream, WebSocketStream,
};

use super::{
    msg::{self, RawMsg},
    server::NodeConnectError,
};

#[derive(Debug)]
pub(crate) struct WsStream {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    peer_address: SocketAddr,
}

impl Stream for WsStream {
    type Item = RawMsg;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.stream
            .poll_next_unpin(cx)
            .map(|val| val.map(|res| RawMsg::new(res.map_err(|e| WsRecvError::Tungstenite(e)))))
    }
}

impl WsStream {
    pub async fn send_challenge(&mut self, msg: msg::Challenge) -> Result<(), WsSendError> {
        self.stream
            .send(Message::Binary(msg::Msg::Challenge(msg).serialize()))
            .await
            .map_err(|e| WsSendError::Tungstenite(e))
    }

    pub async fn send<'a>(&mut self, msg: msg::Msg<'a>) -> Result<(), WsSendError> {
        self.stream
            .send(Message::Binary(msg.serialize()))
            .await
            .map_err(|e| WsSendError::Tungstenite(e))
    }

    pub fn peer_addr(&self) -> &SocketAddr {
        &self.peer_address
    }

    pub(crate) async fn handshake_as_server(
        stream: MaybeTlsStream<TcpStream>,
    ) -> Result<Self, NodeConnectError> {
        let peer_address = Self::_peer_addr_(&stream)?;
        match tokio_tungstenite::accept_async(stream).await {
            Ok(stream) => Ok(Self {
                stream,
                peer_address,
            }),
            Err(e) => Err(NodeConnectError::HandShakeFailed(e)),
        }
    }

    pub(crate) async fn handshake_as_client(
        stream: MaybeTlsStream<TcpStream>,
    ) -> Result<Self, NodeConnectError> {
        let peer_address = Self::_peer_addr_(&stream)?;
        let url = format!("ws://{}", peer_address);

        let (stream, _response) = tokio_tungstenite::client_async(url, stream)
            .await
            .map_err(|e| NodeConnectError::HandShakeFailed(e))?;

        Ok(Self {
            stream,
            peer_address,
        })
    }

    fn _peer_addr_(stream: &MaybeTlsStream<TcpStream>) -> std::io::Result<SocketAddr> {
        match stream {
            MaybeTlsStream::Plain(plain) => plain.peer_addr(),
            MaybeTlsStream::NativeTls(tls) => tls.get_ref().get_ref().get_ref().peer_addr(),
            _ => unreachable!(),
        }
    }


    pub async fn recv(&mut self) -> RawMsg {
        match self.next().await {
            Some(msg) => msg,
            None => RawMsg::new(Err(WsRecvError::StreamClosed)),
        }
    }
}

#[derive(Debug, derive_more::Error, derive_more::Display)]
pub(crate) enum WsRecvError {
    StreamClosed,
    Tungstenite(tungstenite::Error),
    #[error(ignore)]
    NonBinaryMsg(Message),
    #[error(ignore)]
    NotDeserializable,
    #[error(ignore)]
    #[display(fmt = "UnexpectedMsgType({:?})", "0")]
    UnExpectedMsgType,
}

#[derive(Debug, derive_more::Error, derive_more::Display)]
pub enum WsSendError {
    Tungstenite(tungstenite::Error),
}
