use std::net::SocketAddr;

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    tungstenite::{self, Message},
    MaybeTlsStream, WebSocketStream,
};
use uuid::Uuid;

use crate::distributed::server::NodeConnectingError;

use super::{challenge::{Challenge, ChallengeResponse, ChallengeError}, server::InnerNodeConn};

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum Protocol {
    Challenge(Challenge),
    ChallengeReply(ChallengeResponse),
    ChallengeResult(Result<(), ChallengeError>),
    SendNodeId(Uuid)
}

impl Protocol {
    pub(crate) fn serialize(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    pub(crate) fn deserialize(bytes: &[u8]) -> Result<Self, DeserializationError> {
        bincode::deserialize(bytes).map_err(|e| DeserializationError)
    }

    pub(crate) fn deserialize_msg(
        msg: &Option<Result<Message, tungstenite::Error>>,
    ) -> Result<Self, DeserializationError> {
        if let Some(Ok(Message::Binary(bin))) = msg {
            if let Ok(msg) = Protocol::deserialize(bin) {
                Ok(msg)
            } else {
                Err(DeserializationError)
            }
        } else {
            Err(DeserializationError)
        }
    }
}

#[derive(Debug)]
pub(crate) struct DeserializationError;

#[derive(Debug)]
pub(crate) struct ProtocolStream(WebSocketStream<MaybeTlsStream<TcpStream>>);

impl ProtocolStream {
    pub async fn new_server_no_tls(stream: TcpStream) -> Result<Self, NodeConnectingError> {
        let stream = MaybeTlsStream::Plain(stream);

        match tokio_tungstenite::accept_async(stream).await {
            Ok(ws) => Ok(Self(ws)),
            Err(e) => Err(NodeConnectingError::HandShakeFailed(e)),
        }
    }

    pub async fn new_client_no_tls(addr: SocketAddr) -> Result<Self, NodeConnectingError> {
        // connect tcp stream
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| NodeConnectingError::CantConnectToAddr(e))?;

        // create connection url
        let ip = stream.peer_addr().unwrap().ip().to_string();
        let url = format!("ws://{}", ip);

        let stream = MaybeTlsStream::Plain(stream);

        // ws handshake
        let (stream, _response) = tokio_tungstenite::client_async(url, stream)
            .await
            .map_err(|e| NodeConnectingError::HandShakeFailed(e))?;

        Ok(Self(stream))
    }

    pub fn peer_addr(&self) -> std::io::Result<SocketAddr> {
        let refs = self.0.get_ref();
        match refs {
            MaybeTlsStream::Plain(plain) => plain.peer_addr(),
            // _ => panic!("Haven't yet implemented tls, todo!")
            // MaybeTlsStream::NativeTls(a) => (),
            MaybeTlsStream::NativeTls(tls) => {
                tls.get_ref().get_ref().get_ref().peer_addr()
            }
            _ => unreachable!()
        }
    }

    pub async fn send(&mut self, msg: Protocol) -> Result<(), ProtocolSendError> {
        self.0.send(Message::Binary(msg.serialize())).await.map_err(|e|ProtocolSendError::Any(e))
    }

    pub async fn recv_next(&mut self) -> ProtocolRecv {
        match self.0.next().await {
            Some(val) => match val {
                Ok(msg) => match msg {
                    Message::Binary(bin) => match Protocol::deserialize(&bin) {
                        Ok(prot) => ProtocolRecv::Protocol(prot),
                        Err(e) => ProtocolRecv::CouldntDeserialize(bin),
                    },
                    other_msg => ProtocolRecv::NonBinaryMsg(other_msg)
                },
                Err(e) => ProtocolRecv::StreamClosedAbrupt(e),
            },
            None => ProtocolRecv::StreamClosed,
        }
    }
}


#[derive(Debug)]
pub(crate) enum ProtocolRecv {
    Protocol(Protocol),
    StreamClosed,
    StreamClosedAbrupt(tungstenite::Error),
    NonBinaryMsg(Message),
    CouldntDeserialize(Vec<u8>)
}

#[derive(Debug)]
pub(crate) enum ProtocolSendError {
    Any(tungstenite::Error)
}