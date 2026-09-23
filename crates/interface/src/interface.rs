use super::*;
use std::convert::Infallible;
use type_sets::{AsTypeSet, Members};

/// Defines the set of accepted messages, and conversions from/to envelopes.
///
/// While possible to implement manually, it is much easier to [derive](derive@Interface).
///
/// # Example
///
/// ```
/// # use zestors::interface::{Envelope, Interface, Message};
///
/// #[derive(Message, Debug)]
/// # #[zestors(interface_path = "zestors::interface")]
/// struct Ping;
///
/// #[derive(Message, Debug)]
/// #[msg(reply = u32)]
/// # #[zestors(interface_path = "zestors::interface")]
/// struct Double(u32);
///
/// #[derive(Interface, Debug)]
/// # #[zestors(interface_path = "zestors::interface")]
/// enum MyInterface {
///     Ping(Envelope<Ping>),
///     Double(Envelope<Double>),
/// }
///
/// // A concrete envelope round-trips through the interface's type-erased
/// // form - this is what lets an actor's channel accept messages it only
/// // knows about dynamically (e.g. through a `Dyn<(Double,)>` reference).
/// let (envelope, _receipt) = Envelope::new_pair(Double(21));
/// let any_envelope = MyInterface::Double(envelope).into_dyn_envelope();
/// let restored = MyInterface::try_from_dyn_envelope(any_envelope).unwrap();
/// assert!(matches!(restored, MyInterface::Double(_)));
/// ```
pub trait Interface:
    Message<Output = (), Kind = Cast> + TryInto<Envelope<Self>> + From<Envelope<Self>>
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

/// The simplest possible [`Interface`]: it accepts one message, `()`, and
/// nothing else - see `zestors-runtime`'s crate-level example for an actor
/// built on `Inbox<()>`.
impl Interface for () {
    type Set = ((),);

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

/// An [`Interface`] that accepts no messages at all: every conversion is
/// unreachable, since a value of [`Infallible`] can never actually exist.
/// This is what backs `zestors-runtime`'s signal-only actors (its
/// `TaskBox`), which only ever receive signals, never messages.
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
