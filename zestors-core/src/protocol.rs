use crate::*;
use std::any::TypeId;

/// In order for an actor to receive any messages, it must first define a protocol.
/// This protocol defines exactly which [Messages](Message) it can receive, and how
/// to convert these messages into `Self`.
///
/// This is normally derived using [protocol](tiny_actor_codegen::protocol).
pub trait Protocol: Send + 'static {
    /// Attempt to create the `Protocol` from a `BoxedMessage`.
    ///
    /// This should return `Ok(Self)`, if it is downcastable to an accepted message,
    /// or else this should return `Err(BoxedMessage)`.
    fn try_from_boxed(boxed: BoxedMessage) -> Result<Self, BoxedMessage>
    where
        Self: Sized;

    /// Convert the `Protocol` into a `BoxedMessage`.
    ///
    /// This should be implemented by matching on the message type, and then putting
    /// that in the `BoxedMessage`.
    fn into_boxed(self) -> BoxedMessage;

    /// Whether the `Protocol` accepts a certain message.
    ///
    /// The type-id given here is the `TypeId` of the message, and should return true
    /// if this message is actually accepted.
    fn accepts(id: &TypeId) -> bool
    where
        Self: Sized;
}

/// In order for an actor to receive a message, it must implement `Accept<Message>`.
///
/// This is normally derived using [protocol](tiny_actor_codegen::protocol).
pub trait ProtocolMessage<M: Message>: Protocol {
    /// Create the `Protocol` from the sender-part of the message.
    fn from_sends(msg: Sends<M>) -> Self
    where
        Self: Sized;

    /// Attempt to extract a certain message from the `Protocol`.
    ///
    /// This should return the `Ok(Sends<M>)` if the `Protocol` is of that type, otherwise
    /// this should return `Err(Self)`.
    fn try_into_sends(self) -> Result<Sends<M>, Self>
    where
        Self: Sized;

    fn unwrap_into_msg(self, returns: Returns<M>) -> M
    where
        Self: Sized,
    {
        match self.try_into_sends() {
            Ok(sends) => <M::Type as MsgType<M>>::into_msg(sends, returns),
            Err(_) => panic!(),
        }
    }
}

/// In order to send a message to an actor, the the message has to implement [Message].
///
/// This is normally derived using [Message](tiny_actor_codegen::Message).
/// ```
pub trait Message: Sized {
    type Type: MsgType<Self>;
}

/// The `MsgType` of a message indicates the type of message that is sent.
///
/// By default it is only implemented for `()`, which just treats it as a one-off
/// message without reply.
pub trait MsgType<M> {
    /// This is the message that is actually sent to the actor.
    ///
    /// It is called the sender-part of the message. (`Sends<M>`)
    type Sends;

    /// This is that which is returned when a message is sent to an actor.
    ///
    /// It is called the returns-part of the message. (`Returns<M>`)
    type Returns;

    /// Create a new
    fn new_pair(msg: M) -> (Self::Sends, Self::Returns);
    fn into_msg(sends: Self::Sends, returns: Self::Returns) -> M;
}

impl<M> MsgType<M> for () {
    type Sends = M;
    type Returns = ();

    fn new_pair(msg: M) -> (M, ()) {
        (msg, ())
    }

    fn into_msg(sends: M, _returns: ()) -> M {
        sends
    }
}

/// A shorthand for writing `<<M as Message>::Type as MsgType<M>>::Sends`, the
/// sender-part of a message.
pub type Sends<M> = <<M as Message>::Type as MsgType<M>>::Sends;

/// A shorthand for writing `<<M as Message>::Type as MsgType<M>>::Returns`, the
/// returner-part of a message.
pub type Returns<M> = <<M as Message>::Type as MsgType<M>>::Returns;



macro_rules! impl_message {
    ($($ty:ty),*) => {
        $(
            impl Message for $ty {
                type Type = ();
            }
        )*
    };
}

impl_message! {
    u8, u16, u32, u64, u128,
    i8, i16, i32, i64, i128,
    ()
}
