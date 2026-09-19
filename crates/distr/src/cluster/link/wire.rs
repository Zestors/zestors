//! How things are laid out on streams.
//!
//! A stream that carries messages starts with one byte, the protocol they are
//! for, then any number of frames: a length (`u32`, big-endian), then that many
//! bytes. A datagram is the protocol byte, then the message. The first stream
//! on a connection instead carries a [`Hello`] in each direction, as one frame.

use crate::NodeId;
use bytes::{BufMut, Bytes, BytesMut};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The largest message that can be sent or is accepted from a peer.
pub(super) const MAX_MESSAGE_SIZE: usize = 4 * 1024 * 1024;

/// The most a [`Hello`] can take.
const MAX_HELLO_SIZE: usize = 1024;

pub(super) async fn write_frame(
    stream: &mut (impl AsyncWrite + Unpin),
    payload: &[u8],
) -> io::Result<()> {
    stream
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await?;
    stream.write_all(payload).await
}

/// The next frame, or `None` if the stream ended cleanly between two frames.
pub(super) async fn read_frame(
    stream: &mut (impl AsyncRead + Unpin),
    max: usize,
) -> io::Result<Option<Bytes>> {
    let mut len = [0u8; 4];
    if stream.read(&mut len[..1]).await? == 0 {
        return Ok(None);
    }
    stream.read_exact(&mut len[1..]).await?;
    let len = u32::from_be_bytes(len) as usize;
    if len > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message of {len} bytes is over the limit of {max}"),
        ));
    }
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await?;
    Ok(Some(Bytes::from(payload)))
}

/// The first message on every connection, in both directions: who is on the
/// other end.
pub(super) struct Hello {
    pub(super) node: NodeId,
    pub(super) generation: u64,
}

impl Hello {
    fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();
        buf.put_u64(self.generation);
        buf.put_slice(self.node.as_str().as_bytes());
        buf.freeze()
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        let (generation, node) = bytes.split_at_checked(8)?;
        Some(Self {
            node: NodeId::new(std::str::from_utf8(node).ok()?),
            generation: u64::from_be_bytes(generation.try_into().ok()?),
        })
    }
}

pub(super) async fn write_hello(
    stream: &mut (impl AsyncWrite + Unpin),
    hello: &Hello,
) -> io::Result<()> {
    write_frame(stream, &hello.encode()).await?;
    stream.flush().await
}

pub(super) async fn read_hello(stream: &mut (impl AsyncRead + Unpin)) -> io::Result<Hello> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid hello");
    let frame = read_frame(stream, MAX_HELLO_SIZE)
        .await?
        .ok_or_else(invalid)?;
    Hello::decode(&frame).ok_or_else(invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_round_trips() {
        let hello = Hello {
            node: NodeId::new("node-a"),
            generation: 42,
        };
        let decoded = Hello::decode(&hello.encode()).unwrap();
        assert_eq!(decoded.node, hello.node);
        assert_eq!(decoded.generation, 42);
    }

    #[test]
    fn malformed_hellos_are_refused() {
        assert!(Hello::decode(&[]).is_none());
        assert!(Hello::decode(&[0; 4]).is_none());
        let mut not_utf8 = vec![0u8; 8];
        not_utf8.push(0xff);
        assert!(Hello::decode(&not_utf8).is_none());
    }

    #[tokio::test]
    async fn frames_round_trip_and_end_cleanly() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        write_frame(&mut a, b"one").await.unwrap();
        write_frame(&mut a, b"").await.unwrap();
        drop(a);
        assert_eq!(read_frame(&mut b, 16).await.unwrap().unwrap(), "one");
        assert_eq!(read_frame(&mut b, 16).await.unwrap().unwrap(), "");
        assert!(read_frame(&mut b, 16).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn oversized_frames_are_refused() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        write_frame(&mut a, &[0; 32]).await.unwrap();
        assert!(read_frame(&mut b, 16).await.is_err());
    }
}
