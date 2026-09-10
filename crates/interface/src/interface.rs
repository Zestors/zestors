use type_sets::Members;

use super::*;
use std::convert::Infallible;

/// Defines the set of messages that an actor accepts.
pub trait Interface:
    Message<Receipt = ()> + TryInto<Envelope<Self>> + From<Envelope<Self>>
{
    /// The set of messages that this interface can handle. (a tuple)
    type Set: AsTypeSet + Members;

    /// Attempt to convert a boxed envelope into this interface by downcasting.
    fn try_from_dyn_envelope(envelope: AnyEnvelope) -> Result<Self, AnyEnvelope>;

    /// Convert the inner envelope of this interface into a boxed envelope.
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
    fn from(_envelope: Envelope<()>) -> Self {
        ()
    }
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
