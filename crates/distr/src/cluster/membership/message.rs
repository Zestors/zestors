use crate::{Member, NodeName, link::Delivery};
use bytes::{BufMut, Bytes, BytesMut};
use foca::{Codec, PostcardCodec};

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

/// The node that a gossip packet says it is from.
///
/// Members gossip about each other, but every packet also names its own sender,
/// and foca answers whoever that is. So it has to be the node that the packet
/// came from, which the backend has established; otherwise any member could
/// speak for another.
pub(super) fn sender_of(packet: &[u8]) -> Option<NodeName> {
    let header = Codec::<Member>::decode_header(&mut PostcardCodec, packet).ok()?;
    Some(header.src.name)
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

    /// A gossip packet as a node called `from` would send it.
    fn packet_from(from: &str) -> Vec<u8> {
        use foca::{AccumulatingRuntime, Config, Foca};
        use rand::{SeedableRng, rngs::StdRng};

        let member = |name: &str| Member {
            name: NodeName::new(name),
            addr: format!("{name}:7000").into(),
            generation: 1,
        };
        let mut foca = Foca::new(
            member(from),
            Config::simple(),
            StdRng::seed_from_u64(1),
            PostcardCodec,
        );
        let mut runtime = AccumulatingRuntime::new();
        foca.announce(member("node-target"), &mut runtime).unwrap();
        runtime
            .to_send()
            .expect("An announcement is sent")
            .1
            .to_vec()
    }

    #[test]
    fn a_packet_names_its_sender() {
        assert_eq!(
            sender_of(&packet_from("node-a")),
            Some(NodeName::new("node-a"))
        );
        assert_eq!(sender_of(b"not a packet"), None);
    }
}
