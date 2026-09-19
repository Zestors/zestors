//! How a [`Frame`] is laid out on the wire: a kind byte, then the payload.

use crate::cluster::net::Frame;
use bytes::{BufMut, Bytes, BytesMut};

const KIND_GOSSIP: u8 = 1;
const KIND_DEPARTURE: u8 = 2;

impl Frame {
    /// Whether the frame may be sent as an unreliable datagram: loss-tolerant
    /// messages only. Everything else goes over a reliable stream.
    pub(super) fn is_datagram(&self) -> bool {
        matches!(self, Frame::Gossip(_))
    }

    pub(super) fn encode(&self) -> Bytes {
        match self {
            Frame::Gossip(data) => {
                let mut buf = BytesMut::with_capacity(1 + data.len());
                buf.put_u8(KIND_GOSSIP);
                buf.put_slice(data);
                buf.freeze()
            }
            Frame::Departure => Bytes::from_static(&[KIND_DEPARTURE]),
        }
    }

    pub(super) fn decode(mut bytes: Bytes) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        match bytes.split_to(1)[0] {
            KIND_GOSSIP => Some(Frame::Gossip(bytes)),
            KIND_DEPARTURE => Some(Frame::Departure),
            _ => None,
        }
    }
}
