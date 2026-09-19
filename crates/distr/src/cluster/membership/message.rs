use crate::cluster::link::Delivery;
use bytes::{BufMut, Bytes, BytesMut};

const KIND_GOSSIP: u8 = 1;
const KIND_DEPARTURE: u8 = 2;

/// What nodes say to each other about membership, on
/// [`Protocol::MEMBERSHIP`](crate::backend::Protocol::MEMBERSHIP): a kind byte,
/// then the payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Message {
    /// Opaque membership protocol bytes.
    Gossip(Bytes),
    /// The sender is shutting down cleanly and is not going to come back.
    Departure,
}

impl Message {
    /// How it has to be sent: gossip repeats itself and tolerates loss, a
    /// departure is said once.
    pub(super) fn delivery(&self) -> Delivery {
        match self {
            Message::Gossip(_) => Delivery::Datagram,
            Message::Departure => Delivery::Ordered,
        }
    }

    pub(super) fn encode(&self) -> Bytes {
        match self {
            Message::Gossip(data) => {
                let mut buf = BytesMut::with_capacity(1 + data.len());
                buf.put_u8(KIND_GOSSIP);
                buf.put_slice(data);
                buf.freeze()
            }
            Message::Departure => Bytes::from_static(&[KIND_DEPARTURE]),
        }
    }

    pub(super) fn decode(mut bytes: Bytes) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        match bytes.split_to(1)[0] {
            KIND_GOSSIP => Some(Message::Gossip(bytes)),
            KIND_DEPARTURE => Some(Message::Departure),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        for message in [
            Message::Gossip(Bytes::from_static(b"hello")),
            Message::Departure,
        ] {
            assert_eq!(Message::decode(message.encode()), Some(message));
        }
    }

    #[test]
    fn garbage_is_not_a_message() {
        assert_eq!(Message::decode(Bytes::new()), None);
        assert_eq!(Message::decode(Bytes::from_static(&[0xff, 1, 2])), None);
    }
}
