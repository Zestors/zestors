//! The identity exchange that opens every connection.

use super::{BoxError, CLOSE_VERSION, send_message};
use crate::NodeId;
use bytes::{BufMut, Bytes, BytesMut};
use quinn::Connection;

/// The version of the connection protocol (the [`Hello`] exchange and
/// [`Frame`](crate::cluster::net::Frame) encoding). Peers with a different
/// version refuse each other, so bump it on any incompatible change.
pub(super) const PROTOCOL_VERSION: u16 = 1;

pub(super) const KIND_HELLO: u8 = 0;

/// The first message on every connection, in both directions: who is on the
/// other end.
///
/// Layout: kind, protocol version (`u16`), generation (`u64`), node name. The
/// version comes first and is checked before anything after it is parsed, so
/// later versions are free to change the rest.
pub(super) struct Hello {
    pub(super) node: NodeId,
    pub(super) generation: u64,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum HelloError {
    #[error("invalid hello")]
    Invalid,
    #[error("incompatible protocol version {0} (ours is {PROTOCOL_VERSION})")]
    Version(u16),
}

impl Hello {
    pub(super) fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();
        buf.put_u8(KIND_HELLO);
        buf.put_u16(PROTOCOL_VERSION);
        buf.put_u64(self.generation);
        buf.put_slice(self.node.as_str().as_bytes());
        buf.freeze()
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Self, HelloError> {
        let (&kind, rest) = bytes.split_first().ok_or(HelloError::Invalid)?;
        if kind != KIND_HELLO || rest.len() < 2 {
            return Err(HelloError::Invalid);
        }
        let (version, rest) = rest.split_at(2);
        let version = u16::from_be_bytes([version[0], version[1]]);
        if version != PROTOCOL_VERSION {
            return Err(HelloError::Version(version));
        }
        if rest.len() < 8 {
            return Err(HelloError::Invalid);
        }
        let (generation, node) = rest.split_at(8);
        Ok(Self {
            node: NodeId::new(std::str::from_utf8(node).map_err(|_| HelloError::Invalid)?),
            generation: u64::from_be_bytes(generation.try_into().unwrap()),
        })
    }
}

pub(super) async fn send_hello(
    conn: &Connection,
    node: &NodeId,
    generation: u64,
) -> Result<(), BoxError> {
    let hello = Hello {
        node: node.clone(),
        generation,
    };
    send_message(conn, &hello.encode()).await?;
    Ok(())
}

pub(super) async fn read_hello(conn: &Connection) -> Result<Hello, BoxError> {
    let mut stream = conn.accept_uni().await?;
    let bytes = stream.read_to_end(1024).await?;
    match Hello::decode(&bytes) {
        Ok(hello) => Ok(hello),
        Err(err) => {
            if matches!(err, HelloError::Version(_)) {
                conn.close(CLOSE_VERSION.into(), b"incompatible protocol version");
            }
            Err(err.into())
        }
    }
}
