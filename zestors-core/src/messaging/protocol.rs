use crate::*;
use std::any::TypeId;

//------------------------------------------------------------------------------------------------
//  Protocol
//------------------------------------------------------------------------------------------------

/// In order for an actor to receive any messages, it must first define a [Protocol].
/// This protocol defines exactly which [Messages](Message) the actor can receive, and how
/// to convert these messages into the [Protocol] that is received by the [Inbox].
///
/// This is normally derived using the [protocol] macro, but may be implemented manually.
pub trait Protocol: Send + 'static {
    /// Convert the [Protocol] into a [BoxedMessage].
    ///
    /// This should be implemented by matching on the message type creating a [BoxedMessage]
    /// from that.
    fn boxed(self) -> BoxedMessage;

    /// Attempt to create the [Protocol] from a [BoxedMessage].
    ///
    /// This should return `Ok(Self)`, if it is downcast-able to an accepted message.
    /// Otherwise this should return `Err(BoxedMessage)`.
    fn try_unbox(boxed: BoxedMessage) -> Result<Self, BoxedMessage>
    where
        Self: Sized;

    /// Whether the [Protocol] accepts a message.
    ///
    /// The type-id given here is the `TypeId` of the message, and should return true
    /// if this message is accepted.
    fn accepts(id: &TypeId) -> bool
    where
        Self: Sized;
}

//------------------------------------------------------------------------------------------------
//  ProtocolMessage
//------------------------------------------------------------------------------------------------

/// In order to send message `M` to an actor, it's protocol must implement `ProtocolMessage<M>`.
///
/// This is normally derived using the [protocol] macro, but may be implemented manually.
pub trait ProtocolMessage<M: Message>: Protocol {
    /// Create the [Protocol] from `Sends<M>`.
    fn from_sends(msg: SendPart<M>) -> Self
    where
        Self: Sized;

    /// Attempt to extract message `M` from the [Protocol].
    ///
    /// This should return the `Ok(Sends<M>)` if the [Protocol] is of that type, otherwise
    /// this should return `Err(Self)`.
    fn try_into_sends(self) -> Result<SendPart<M>, Self>
    where
        Self: Sized;
}

//------------------------------------------------------------------------------------------------
//  ProtocolMessageExt
//------------------------------------------------------------------------------------------------

/// A helper trait
pub(crate) trait ProtocolMessageExt<M: Message>: ProtocolMessage<M> {
    fn unwrap_into_msg(self, returns: ReturnPart<M>) -> M;
}

impl<M: Message, T> ProtocolMessageExt<M> for T
where
    T: ProtocolMessage<M>,
{
    fn unwrap_into_msg(self, returns: ReturnPart<M>) -> M {
        match self.try_into_sends() {
            Ok(sends) => <M::Type as MessageType<M>>::destroy(sends, returns),
            Err(_) => panic!(),
        }
    }
}
