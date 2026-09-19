use super::*;
use std::any::Any;

/// Pairs a [`Message`] with the [`Resolver`] used to answer it.
///
/// # Example
///
/// ```
/// # use zestors::interface::{Envelope, Message, Receipt as _};
///
/// #[derive(Message, Debug)]
/// #[msg(reply = u32)]
/// # #[zestors(interface_path = "zestors::interface")]
/// struct Double(u32);
///
/// # #[tokio::main]
/// # async fn main() {
/// let (envelope, receipt) = Envelope::new_pair(Double(21));
///
/// let n = envelope.msg.0;
/// envelope.reply(n * 2).unwrap();
///
/// assert_eq!(receipt.wait().await.unwrap(), 42);
/// # }
/// ```
#[derive(Debug)]
pub struct Envelope<M: Message> {
    /// The message payload.
    pub msg: M,

    /// The resolver handle used by the receiver to resolve the message's outcome.
    pub req: ResolverOf<M>,
}

impl<M: Message> Envelope<M> {
    /// Creates an envelope containing a message and its resolver.
    pub fn new(msg: M, req: ResolverOf<M>) -> Self {
        Self { msg, req }
    }

    /// Creates an envelope containing `msg` together with a freshly
    /// constructed resolver, and returns the matching receipt alongside it.
    pub fn new_pair(msg: M) -> (Self, ReceiptOf<M>) {
        let (resolver, receipt) = <ResolverOf<M> as Resolver>::new();
        (Self::new(msg, resolver), receipt)
    }

    /// Resolves the envelope's resolver with `reply`, for messages whose
    /// resolver is a [`Request<T>`].
    pub fn reply<T: Send + 'static>(self, reply: T) -> Result<(), ResolveError<T>>
    where
        M: Message<Output = T, Kind = Call>,
    {
        self.req.reply(reply)
    }
}

/// A type-erased [`Envelope`] used for dynamic sending.
///
/// # Example
///
/// ```
/// # use zestors::interface::{AnyEnvelope, Message};
///
/// #[derive(Message, Debug, PartialEq)]
/// # #[zestors(interface_path = "zestors::interface")]
/// struct Ping;
///
/// #[derive(Message, Debug, PartialEq)]
/// # #[zestors(interface_path = "zestors::interface")]
/// struct Pong;
///
/// let (any_envelope, _receipt) = AnyEnvelope::new_pair(Ping);
///
/// // Downcasting to the wrong message type hands the envelope back
/// // unchanged instead of losing it.
/// let any_envelope = any_envelope.downcast::<Pong>().unwrap_err();
/// let envelope = any_envelope.downcast::<Ping>().unwrap();
/// assert_eq!(envelope.msg, Ping);
/// ```
#[derive(Debug)]
pub struct AnyEnvelope(Box<dyn Any + Send>);

impl AnyEnvelope {
    /// Creates a new `AnyEnvelope` from an [`Envelope`].
    pub fn new<M: Message>(envelope: Envelope<M>) -> Self {
        Self(Box::new(envelope))
    }

    /// Creates a type-erased `AnyEnvelope` containing `msg` together with a
    /// freshly constructed resolver, and returns the matching receipt
    /// alongside it.
    pub fn new_pair<M: Message>(msg: M) -> (Self, ReceiptOf<M>) {
        let (envelope, receipt) = Envelope::new_pair(msg);
        (Self::new(envelope), receipt)
    }

    /// Downcasts to a specific [`Envelope<M>`], returning `Err(self)`
    /// unchanged if this envelope does not hold an `M`.
    pub fn downcast<M: Message>(self) -> Result<Envelope<M>, Self> {
        match self.0.downcast() {
            Ok(cast) => Ok(*cast),
            Err(boxed) => Err(Self(boxed)),
        }
    }
}
