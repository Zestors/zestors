use super::*;
use std::convert::Infallible;
use type_sets::{AsTypeSet, Members};

/// Defines the set of accepted messages, and conversions from/to envelopes.
///
/// While possible to implement manually, it is much easier to [derive](derive@Interface).
pub trait Interface:
    Message<Receipt = ()> + TryInto<Envelope<Self>> + From<Envelope<Self>>
{
    /// A type-level set — implemented as a tuple — of the message types this
    /// interface accepts.
    type Set: AsTypeSet + Members;

    /// Attempts to convert a type-erased [`AnyEnvelope`] into this interface
    /// by downcasting, returning `Err` unchanged if the envelope holds a
    /// message type not accepted by this interface.
    fn try_from_dyn_envelope(envelope: AnyEnvelope) -> Result<Self, AnyEnvelope>;

    /// Converts this interface's inner envelope into a type-erased [`AnyEnvelope`].
    fn into_dyn_envelope(self) -> AnyEnvelope;
}

impl Interface for () {
    type Set = ();

    fn try_from_dyn_envelope(envelope: AnyEnvelope) -> Result<Self, AnyEnvelope> {
        envelope.downcast::<()>().map(|env| env.msg)
    }

    fn into_dyn_envelope(self) -> AnyEnvelope {
        AnyEnvelope::new::<()>(Envelope::new(self, ()))
    }
}

impl From<Envelope<()>> for () {
    fn from(_envelope: Envelope<()>) -> Self {}
}

impl TryInto<Envelope<()>> for () {
    type Error = Self;

    fn try_into(self) -> Result<Envelope<()>, Self> {
        Ok(Envelope::new((), ()))
    }
}

impl Interface for Infallible {
    type Set = ();

    fn try_from_dyn_envelope(envelope: AnyEnvelope) -> Result<Self, AnyEnvelope> {
        Err(envelope)
    }

    fn into_dyn_envelope(self) -> AnyEnvelope {
        unreachable!()
    }
}

impl From<Envelope<Infallible>> for Infallible {
    fn from(_envelope: Envelope<Infallible>) -> Self {
        unreachable!()
    }
}

impl TryInto<Envelope<Infallible>> for Infallible {
    type Error = Self;

    fn try_into(self) -> Result<Envelope<Infallible>, Self> {
        unreachable!()
    }
}
