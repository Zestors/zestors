use std::net::SocketAddr;

use futures::{SinkExt, Stream, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    tungstenite::{self, Message},
    MaybeTlsStream, WebSocketStream,
};

use super::{server::NodeSetupError, ws_message::WsMsg};

#[derive(Debug)]
pub(crate) struct WsStream(WebSocketStream<MaybeTlsStream<TcpStream>>);

impl WsStream {
    pub async fn handshake_as_server(
        stream: MaybeTlsStream<TcpStream>,
    ) -> Result<Self, NodeSetupError> {
        match tokio_tungstenite::accept_async(stream).await {
            Ok(ws) => Ok(Self(ws)),
            Err(e) => Err(NodeSetupError::HandShakeFailed(e)),
        }
    }

    pub async fn handshake_as_client(
        stream: MaybeTlsStream<TcpStream>,
    ) -> Result<Self, NodeSetupError> {
        let addr = Self::_peer_addr_(&stream).unwrap();
        let url = format!("ws://{}", addr);

        let (stream, _response) = tokio_tungstenite::client_async(url, stream)
            .await
            .map_err(|e| NodeSetupError::HandShakeFailed(e))?;

        Ok(Self(stream))
    }

    fn _peer_addr_(stream: &MaybeTlsStream<TcpStream>) -> std::io::Result<SocketAddr> {
        match stream {
            MaybeTlsStream::Plain(plain) => plain.peer_addr(),
            MaybeTlsStream::NativeTls(tls) => tls.get_ref().get_ref().get_ref().peer_addr(),
            _ => unreachable!(),
        }
    }

    pub fn peer_addr(&self) -> std::io::Result<SocketAddr> {
        Self::_peer_addr_(self.0.get_ref())
    }

    pub async fn send(&mut self, msg: WsMsg) -> Result<(), WsSendError> {
        self.0
            .send(Message::Binary(msg.serialize()))
            .await
            .map_err(|e| WsSendError::Tungstenite(e))
    }

    pub async fn recv_next(&mut self) -> WsRecvResult {
        match self.0.next().await {
            Some(val) => match val {
                Ok(msg) => match msg {
                    Message::Binary(bin) => match WsMsg::deserialize(&bin) {
                        Ok(msg) => Ok(msg),
                        Err(_) => Err(WsRecvError::NotDeserializable(bin)),
                    },
                    other_msg => Err(WsRecvError::NonBinaryMsg(other_msg)),
                },
                Err(e) => Err(WsRecvError::Tungstenite(e)),
            },
            None => Err(WsRecvError::StreamClosed),
        }
    }
}

pub(crate) type WsRecvResult = Result<WsMsg, WsRecvError>;

#[derive(Debug)]
pub enum WsRecvError {
    StreamClosed,
    Tungstenite(tungstenite::Error),
    NonBinaryMsg(Message),
    NotDeserializable(Vec<u8>),
}

#[derive(Debug)]
pub enum WsSendError {
    Tungstenite(tungstenite::Error),
}
