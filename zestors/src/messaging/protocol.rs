use super::*;
use std::{any::TypeId, sync::Arc};

/// This trait must be implemented for all channels through which (messages)[Message] will be sent.
///
/// For every message `M` that the protocol accepts, it should also implement [`ProtocolFrom<M>`].
pub trait Protocol: Send + 'static {
    /// Take out the inner message.
    fn into_boxed_payload(self) -> BoxPayload;

    /// Attempt to create the protocol from a message.
    ///
    /// This succeeds if the protocol implements [`ProtocolFrom<M>`] (i.e. accepts) the message.
    fn try_from_boxed_payload(payload: BoxPayload) -> Result<Self, BoxPayload>
    where
        Self: Sized;

    /// Whether the protocol implements [`ProtocolFrom<M>`] (i.e. accepts) the [Message]'s type-id.
    fn accepts_msg(msg_id: &TypeId) -> bool
    where
        Self: Sized;
}

/// The trait [`ProtocolFrom<M>`] should be implemented for all messages `M` that a
/// [Protocol] accepts.
pub trait FromPayload<M: Message>: Protocol {
    /// Create the protocol from the payload of a message.
    fn from_payload(payload: M::Payload) -> Self
    where
        Self: Sized;

    /// Attempt to convert the protocol into a specific payload of a message.
    ///
    /// This succeeds if the inner message of the protocol is of the same type.
    fn try_into_payload(self) -> Result<M::Payload, Self>
    where
        Self: Sized;
}

//------------------------------------------------------------------------------------------------
//  Protocol: `()`
//------------------------------------------------------------------------------------------------

impl Protocol for () {
    fn into_boxed_payload(self) -> BoxPayload {
        BoxPayload::new::<()>(())
    }

    fn try_from_boxed_payload(payload: BoxPayload) -> Result<Self, BoxPayload> {
        payload.downcast::<()>()
    }

    fn accepts_msg(msg_id: &std::any::TypeId) -> bool {
        *msg_id == TypeId::of::<()>()
    }
}

impl FromPayload<()> for () {
    fn from_payload(payload: ()) -> Self
    where
        Self: Sized,
    {
        payload
    }

    fn try_into_payload(self) -> Result<(), Self>
    where
        Self: Sized,
    {
        Ok(self)
    }
}

impl Protocol for Arc<()> {
    fn into_boxed_payload(self) -> BoxPayload {
        BoxPayload::new::<Arc<()>>(self)
    }

    fn try_from_boxed_payload(boxed: BoxPayload) -> Result<Self, BoxPayload> {
        boxed.downcast::<Arc<()>>()
    }

    fn accepts_msg(msg_id: &std::any::TypeId) -> bool {
        *msg_id == TypeId::of::<Arc<()>>()
    }
}

impl FromPayload<Arc<()>> for Arc<()> {
    fn from_payload(msg: <Arc<()> as Message>::Payload) -> Self
    where
        Self: Sized,
    {
        msg
    }

    fn try_into_payload(self) -> Result<<Arc<()> as Message>::Payload, Self>
    where
        Self: Sized,
    {
        Ok(self)
    }
}
