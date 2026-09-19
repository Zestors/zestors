//! What a message must be to cross the network, and how its receipt comes back.

use super::{
    Decode, Encode,
    reply::{RemoteReceipt, RemoteReply},
};
use crate::StableId;
use std::time::Duration;
use zestors_interface::{Call, Cast, Message, MessageKind, ReceiptOf};

/// A [`Message`] that can be sent to an actor on another node.
///
/// There is nothing to implement: every message that has a [`StableId`], can be
/// [`Encode`]d and [`Decode`]d, and whose reply ([`Message::Output`]) can be too,
/// is one. With serde that is
/// `#[derive(Message, StableId, Serialize, Deserialize)]`.
pub trait RemoteMessage:
    Message<Output: Encode + Decode, Kind: RemoteMessageKind<Self::Output>> + StableId + Encode + Decode
{
    /// What sending the message gives back to wait on, like
    /// [`Message::Receipt`]: `()` if it expects no reply, else a
    /// [`RemoteReply`](super::RemoteReply).
    type RemoteReceipt: RemoteReceipt<Output = Self::Output>;

    /// The [`RemoteReceipt`](Self::RemoteReceipt), given what to wait on if
    /// there is a reply.
    fn remote_receipt(waiting: Option<RemoteReply<Self::Output>>) -> Self::RemoteReceipt;

    /// What to wait on for a message sent to an actor on this node.
    fn local_receipt(receipt: ReceiptOf<Self>, timeout: Option<Duration>) -> Self::RemoteReceipt;
}

impl<M> RemoteMessage for M
where
    M: Message<Output: Encode + Decode, Kind: RemoteMessageKind<M::Output>>
        + StableId
        + Encode
        + Decode,
{
    type RemoteReceipt = <Self::Kind as RemoteMessageKind<M::Output>>::RemoteReceipt;

    fn remote_receipt(waiting: Option<RemoteReply<M::Output>>) -> Self::RemoteReceipt {
        <Self::Kind as RemoteMessageKind<M::Output>>::remote(waiting)
    }

    fn local_receipt(receipt: ReceiptOf<M>, timeout: Option<Duration>) -> Self::RemoteReceipt {
        <Self::Kind as RemoteMessageKind<M::Output>>::local(receipt, timeout)
    }
}

/// How the [`Receipt`](zestors_interface::Receipt) of a message, `()` or
/// [`Reply<T>`](zestors_interface::Reply), is sent and received remotely. The
/// two are all there are.
pub trait RemoteMessageKind<T>: MessageKind<T> {
    type RemoteReceipt: RemoteReceipt<Output = T>;

    /// Whether the message gets a reply.
    const REPLIES: bool;

    /// The remote receipt, given what to wait on if there is a reply.
    fn remote(waiting: Option<RemoteReply<T>>) -> Self::RemoteReceipt;

    /// What is waited on for a message sent to an actor on this node.
    fn local(receipt: Self::Receipt, timeout: Option<Duration>) -> Self::RemoteReceipt;
}

impl RemoteMessageKind<()> for Cast {
    type RemoteReceipt = ();
    const REPLIES: bool = false;

    fn remote(_: Option<RemoteReply<()>>) {}

    fn local(_: (), _: Option<Duration>) {}
}

impl<T: Send + 'static> RemoteMessageKind<T> for Call {
    type RemoteReceipt = RemoteReply<T>;
    const REPLIES: bool = true;

    fn remote(waiting: Option<RemoteReply<T>>) -> RemoteReply<T> {
        waiting.expect("A message with a reply is sent as a call")
    }

    fn local(receipt: Self::Receipt, timeout: Option<Duration>) -> RemoteReply<T> {
        RemoteReply::local(receipt, timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DecodeError, EncodeError};
    use bytes::Bytes;
    use serde::{Deserialize, Serialize};

    /// A message and reply that go over the wire with serde, for free.
    #[derive(Message, StableId, Serialize, Deserialize, Debug, PartialEq)]
    #[msg(reply = u32, id = "6f1d1b4e-6f2e-4a55-9c4a-3f0d1c2e7a01")]
    #[zestors(interface_path = "zestors_interface", distr_path = "crate")]
    struct Double(u32);

    /// Neither this message nor its reply implements serde: both are encoded
    /// by hand, in a format of their own. `Message` has no serde bound.
    #[derive(Message, StableId, Debug, PartialEq)]
    #[msg(reply = Reversed, id = "0c7d3f0a-2a53-4d0e-8a51-7f6f5f4d9b02")]
    #[zestors(interface_path = "zestors_interface", distr_path = "crate")]
    struct Reverse(String);

    #[derive(Debug, PartialEq)]
    struct Reversed(String);

    impl Encode for Reverse {
        fn encode(&self) -> Result<Bytes, EncodeError> {
            Ok(Bytes::from(format!("reverse:{}", self.0)))
        }
    }

    impl Decode for Reverse {
        fn decode(bytes: Bytes) -> Result<Self, DecodeError> {
            let text = String::from_utf8(bytes.to_vec()).map_err(DecodeError::new)?;
            let text = text
                .strip_prefix("reverse:")
                .ok_or_else(|| DecodeError::new("Not a reverse message"))?;
            Ok(Reverse(text.to_owned()))
        }
    }

    impl Encode for Reversed {
        fn encode(&self) -> Result<Bytes, EncodeError> {
            Ok(Bytes::from(format!("reversed:{}", self.0)))
        }
    }

    impl Decode for Reversed {
        fn decode(bytes: Bytes) -> Result<Self, DecodeError> {
            let text = String::from_utf8(bytes.to_vec()).map_err(DecodeError::new)?;
            let text = text
                .strip_prefix("reversed:")
                .ok_or_else(|| DecodeError::new("Not a reversed reply"))?;
            Ok(Reversed(text.to_owned()))
        }
    }

    /// Compiles only for messages that can cross the network.
    fn assert_remote<M: RemoteMessage>() {}

    #[test]
    fn serde_types_are_remote_messages() {
        assert_remote::<Double>();

        let bytes = Double(21).encode().unwrap();
        assert_eq!(Double::decode(bytes).unwrap(), Double(21));
        let reply = 42u32.encode().unwrap();
        assert_eq!(u32::decode(reply).unwrap(), 42);
    }

    #[test]
    fn types_without_serde_are_remote_messages_with_hand_written_codecs() {
        assert_remote::<Reverse>();

        let bytes = Reverse("abc".into()).encode().unwrap();
        assert_eq!(&bytes[..], b"reverse:abc");
        assert_eq!(Reverse::decode(bytes).unwrap(), Reverse("abc".into()));

        let reply = Reversed("cba".into()).encode().unwrap();
        assert_eq!(Reversed::decode(reply).unwrap(), Reversed("cba".into()));
    }

    #[test]
    fn garbage_fails_to_decode() {
        assert!(Double::decode(Bytes::from_static(&[0xff; 12])).is_err());
        assert!(Reverse::decode(Bytes::from_static(b"nonsense")).is_err());
    }
}
