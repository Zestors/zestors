use super::*;
use std::any::TypeId;

/// This trait must be implemented for all channels through which (messages)[Message] will be sent.
///
/// For every message `M` that the protocol accepts, it should also implement [`ProtocolFrom<M>`].
pub trait Protocol: Send + 'static {
    /// Take out the inner message.
    fn into_msg(self) -> AnyMessage;

    /// Attempt to create the protocol from a message.
    ///
    /// This succeeds if the protocol implements [`ProtocolFrom<M>`] (i.e. accepts) the message.
    fn try_from_msg(msg: AnyMessage) -> Result<Self, AnyMessage>
    where
        Self: Sized;

    /// Whether the protocol implements [`ProtocolFrom<M>`] (i.e. accepts) the [Message]'s type-id.
    fn accepts_msg(msg_id: &TypeId) -> bool
    where
        Self: Sized;
}

/// The trait [`ProtocolFrom<M>`] should be implemented for all messages `M` that a
/// [Protocol] accepts.
pub trait ProtocolFrom<M: Message>: Protocol {
    /// Create the protocol from the payload of a message.
    fn from_msg(msg: M::Payload) -> Self
    where
        Self: Sized;

    /// Attempt to convert the protocol into a specific payload of a message.
    ///
    /// This succeeds it the inner message of the protocol is of the same type.
    fn try_into_msg(self) -> Result<M::Payload, Self>
    where
        Self: Sized;
}

//------------------------------------------------------------------------------------------------
//  Protocol: `()`
//------------------------------------------------------------------------------------------------

impl Protocol for () {
    fn into_msg(self) -> AnyMessage {
        AnyMessage::new::<()>(())
    }

    fn try_from_msg(boxed: AnyMessage) -> Result<Self, AnyMessage> {
        boxed.downcast::<()>()
    }

    fn accepts_msg(msg_id: &std::any::TypeId) -> bool {
        *msg_id == TypeId::of::<()>()
    }
}

impl ProtocolFrom<()> for () {
    fn from_msg(msg: ()) -> Self
    where
        Self: Sized,
    {
        msg
    }

    fn try_into_msg(self) -> Result<(), Self>
    where
        Self: Sized,
    {
        Ok(self)
    }
}
