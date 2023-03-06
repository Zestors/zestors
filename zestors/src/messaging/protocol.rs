use async_trait::async_trait;

use super::*;
use std::any::TypeId;

/// A [`Protocol`] defines exactly which [messages](Message) an actor [`Accepts`]. Normally this is derived with
/// the [`protocol!`] macro.
///
/// [`Protocol`] is automatically implemented for `()`, which accepts only `()`.
pub trait Protocol: Send + 'static {
    /// Take out the payload as a [`BoxPayload`].
    fn into_boxed_payload(self) -> BoxPayload;

    /// Attempt to create the [`Protocol`] from a message.
    ///
    /// # Implementation
    /// This should succeed if and only if the protocol implements [`FromPayload<M>`].
    fn try_from_boxed_payload(payload: BoxPayload) -> Result<Self, BoxPayload>
    where
        Self: Sized;

    /// Whether the [`Protocol`] accepts the type-id of a [`Message`].
    ///
    /// # Implementation
    /// Should return true if and only if the protocol implements [`FromPayload<M>`].
    fn accepts_msg(msg_id: &TypeId) -> bool
    where
        Self: Sized;
}

/// Specifies that a [`Protocol`] can be created from the [`Message::Payload`] of `M`.
pub trait FromPayload<M: Message>: Protocol {
    /// Create the [`Protocol`] from the [`Message::Payload`].
    fn from_payload(payload: M::Payload) -> Self
    where
        Self: Sized;

    /// Attempt to convert the protocol into the [`Message::Payload`] of `M`.
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

#[async_trait]
impl<H: Handler> HandledBy<H> for ()
where
    H: HandleMessage<()>,
{
    async fn handle_with(
        self,
        handler: &mut H,
        state: &mut <H as Handler>::State,
    ) -> HandlerResult<H> {
        handler.handle_msg(state, self).await
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
