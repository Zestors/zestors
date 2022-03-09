use std::{net::SocketAddr, task::Poll};

use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, Stream, StreamExt,
};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    tungstenite::{self, Message},
    MaybeTlsStream, WebSocketStream,
};

use super::{msg, server::NodeConnectError};

#[derive(Debug)]
pub(crate) struct WsStream {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    peer_address: SocketAddr,
}

impl Stream for WsStream {
    type Item = Result<msg::Msg, WsRecvError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.stream.poll_next_unpin(cx) {
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(val)) => match val {
                Ok(msg) => match msg {
                    Message::Binary(bin) => match msg::Msg::deserialize(&bin) {
                        Ok(msg) => Poll::Ready(Some(Ok(msg))),
                        Err(_) => Poll::Ready(Some(Err(WsRecvError::NotDeserializable(bin)))),
                    },
                    other_msg => Poll::Ready(Some(Err(WsRecvError::NonBinaryMsg(other_msg)))),
                },
                Err(e) => Poll::Ready(Some(Err(WsRecvError::Tungstenite(e)))),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

impl WsStream {
    pub async fn send_challenge(&mut self, msg: msg::Challenge) -> Result<(), WsSendError> {
        self.stream
            .send(Message::Binary(msg::Msg::Challenge(msg).serialize()))
            .await
            .map_err(|e| WsSendError::Tungstenite(e))
    }

    pub async fn send(&mut self, msg: msg::Msg) -> Result<(), WsSendError> {
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
            Ok(stream) => {
                Ok(
                    Self {
                        stream,
                        peer_address,
                    },
                )
            }
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

        Ok(
            Self {
                stream,
                peer_address,
            },
        )
    }

    fn _peer_addr_(stream: &MaybeTlsStream<TcpStream>) -> std::io::Result<SocketAddr> {
        match stream {
            MaybeTlsStream::Plain(plain) => plain.peer_addr(),
            MaybeTlsStream::NativeTls(tls) => tls.get_ref().get_ref().get_ref().peer_addr(),
            _ => unreachable!(),
        }
    }

    pub async fn recv_challenge(&mut self) -> Result<msg::Challenge, WsRecvError> {
        match self.stream.next().await {
            Some(val) => match val {
                Ok(msg) => match msg {
                    Message::Binary(bin) => match msg::Msg::deserialize(&bin) {
                        Ok(msg) => match msg {
                            msg::Msg::Challenge(msg) => Ok(msg),
                            other => Err(WsRecvError::UnExpectedMsgType(other)),
                        },
                        Err(_) => Err(WsRecvError::NotDeserializable(bin)),
                    },
                    other_msg => Err(WsRecvError::NonBinaryMsg(other_msg)),
                },
                Err(e) => Err(WsRecvError::Tungstenite(e)),
            },
            None => Err(WsRecvError::StreamClosed),
        }
    }

    pub async fn recv(&mut self) -> Result<msg::Msg, WsRecvError> {
        match self.stream.next().await {
            Some(val) => match val {
                Ok(msg) => match msg {
                    Message::Binary(bin) => match msg::Msg::deserialize(&bin) {
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

#[derive(Debug)]
pub(crate) enum WsRecvError {
    StreamClosed,
    Tungstenite(tungstenite::Error),
    NonBinaryMsg(Message),
    NotDeserializable(Vec<u8>),
    UnExpectedMsgType(msg::Msg),
}


#[derive(Debug, derive_more::Error, derive_more::Display)]
pub enum WsSendError {
    Tungstenite(tungstenite::Error),
}

