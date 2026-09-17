use super::*;
use std::any::Any;

/// Pairs a [`Message`] with the [`Resolver`] used to answer it.
#[derive(Debug)]
pub struct Envelope<M: Message> {
    /// The message payload.
    pub msg: M,

    /// The resolver handle used by the receiver to resolve the message's outcome.
    pub req: M::Resolver,
}

impl<M: Message> Envelope<M> {
    /// Creates an envelope containing a message and its resolver.
    pub fn new(msg: M, req: M::Resolver) -> Self {
        Self { msg, req }
    }

    /// Creates an envelope containing `msg` together with a freshly
    /// constructed resolver, and returns the matching receipt alongside it.
    pub fn new_pair(msg: M) -> (Self, M::Receipt) {
        let (resolver, receipt) = <M::Resolver as Resolver>::new();
        (Self::new(msg, resolver), receipt)
    }

    /// Resolves the envelope's resolver with `reply`, for messages whose
    /// resolver is a [`Request<T>`].
    pub fn reply<T>(self, reply: T) -> Result<(), ResolveError<T>>
    where
        M: Message<Resolver = Request<T>>,
    {
        self.req.reply(reply)
    }
}

/// A type-erased [`Envelope`] used for dynamic sending.
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
    pub fn new_pair<M: Message>(msg: M) -> (Self, M::Receipt) {
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
