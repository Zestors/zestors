use type_sets::Members;

use super::*;
use std::convert::Infallible;

/// An interface defines the set of messages that can be sent to a given actor.
/// This is usually derived on an enum using the `#[derive(Interface)]` macro.
///
/// It defines conversion methods to and from a boxed envelope, which is used for dynamic dispatch of messages.
pub trait Interface:
    Message<Mode = FireAndForget, Outcome = ()> + TryInto<Envelope<Self>> + From<Envelope<Self>>
{
    /// The [set](TypeSet) of messages that this interface can handle.
    type Set: AsTypeSet + Members;

    /// Attempt to convert a boxed envelope into this interface by downcasting.
    fn try_from_dyn_envelope(envelope: DynEnvelope) -> Result<Self, DynEnvelope>;

    /// Convert the inner envelope of this interface into a boxed envelope.
    fn into_dyn_envelope(self) -> DynEnvelope;
}

impl Interface for () {
    type Set = Set<()>;

    fn try_from_dyn_envelope(envelope: DynEnvelope) -> Result<Self, DynEnvelope> {
        envelope.downcast::<()>().map(|env| env.msg)
    }

    fn into_dyn_envelope(self) -> DynEnvelope {
        DynEnvelope::new::<()>(Envelope::new(self, ()))
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
    type Set = Set<()>;

    fn try_from_dyn_envelope(envelope: DynEnvelope) -> Result<Self, DynEnvelope> {
        Err(envelope)
    }

    fn into_dyn_envelope(self) -> DynEnvelope {
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
