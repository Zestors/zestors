//! What a message must be to cross the network, and how its receipt comes back.

use super::{
    Decode, Encode,
    reply::{RemoteReceipt, RemoteReply},
};
use crate::{MessageId, StableId};
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

/// A set of message types — a tuple, or an [`Interface`](zestors_interface::Interface)'s
/// [`Set`](zestors_interface::Interface::Set) — of which every one can be sent
/// to an actor on another node.
///
/// This is what [`Cluster::address`](crate::Cluster::address) needs in order to
/// name the messages to the node the actor is on: each is asked about by its
/// [`MessageId`], which only a [`RemoteMessage`] has. There is nothing to
/// implement; it holds for every tuple of up to 24 remote messages.
///
/// An interface with a message that can't cross the network is therefore not
/// addressable as a whole. Reach the rest of it with
/// [`Cluster::address_dyn`](crate::Cluster::address_dyn):
///
/// ```compile_fail
/// # use zestors_distr::{Cluster, GlobalName};
/// # use zestors_interface::Message;
/// // An ordinary message, but with no `StableId` to name it by and no way to
/// // encode it, so it can only be delivered on this node.
/// #[derive(Message)]
/// # #[zestors(interface_path = "zestors_interface")]
/// struct Local(u32);
///
/// # async fn example(cluster: Cluster, target: GlobalName) {
/// // `Local` is not a `RemoteMessage`, so this does not compile.
/// let _ = cluster.address_dyn::<(Local,)>(target).await;
/// # }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` contains a message that can't be sent to another node",
    label = "not every message here is a `RemoteMessage`",
    note = "a message crosses the network when it has a `StableId` and can be encoded and decoded",
    note = "to address only part of an interface, use `Cluster::address_dyn` with the messages that can"
)]
pub trait RemoteSet {
    /// The id each message in the set goes by on the wire.
    const MESSAGE_IDS: &'static [MessageId];
}

macro_rules! impl_remote_set {
    ($($member:ident),*) => {
        impl<$($member: RemoteMessage,)*> RemoteSet for ($($member,)*) {
            const MESSAGE_IDS: &'static [MessageId] = &[$($member::Id,)*];
        }
    };
}

// The same arity that `type_sets` supports for a set.
impl_remote_set!();
impl_remote_set!(M1);
impl_remote_set!(M1, M2);
impl_remote_set!(M1, M2, M3);
impl_remote_set!(M1, M2, M3, M4);
impl_remote_set!(M1, M2, M3, M4, M5);
impl_remote_set!(M1, M2, M3, M4, M5, M6);
impl_remote_set!(M1, M2, M3, M4, M5, M6, M7);
impl_remote_set!(M1, M2, M3, M4, M5, M6, M7, M8);
impl_remote_set!(M1, M2, M3, M4, M5, M6, M7, M8, M9);
impl_remote_set!(M1, M2, M3, M4, M5, M6, M7, M8, M9, M10);
impl_remote_set!(M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11);
impl_remote_set!(M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12);
impl_remote_set!(M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13);
impl_remote_set!(M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14);
impl_remote_set!(
    M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15
);
impl_remote_set!(
    M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15, M16
);
impl_remote_set!(
    M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15, M16, M17
);
impl_remote_set!(
    M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15, M16, M17, M18
);
impl_remote_set!(
    M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15, M16, M17, M18, M19
);
impl_remote_set!(
    M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15, M16, M17, M18, M19, M20
);
impl_remote_set!(
    M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15, M16, M17, M18, M19, M20, M21
);
impl_remote_set!(
    M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15, M16, M17, M18, M19, M20, M21,
    M22
);
impl_remote_set!(
    M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15, M16, M17, M18, M19, M20, M21,
    M22, M23
);
impl_remote_set!(
    M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15, M16, M17, M18, M19, M20, M21,
    M22, M23, M24
);

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
