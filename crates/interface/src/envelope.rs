use super::*;
use std::any::Any;

/// Contains both the [`Message`] and the [`Resolver`].
#[derive(Debug)]
pub struct Envelope<M: Message> {
    /// The message
    pub msg: M,

    /// The resolver handle used by the receiver to resolve the message's outcome.
    pub request: M::Resolver,
}

impl<M: Message> Envelope<M> {
    /// Creates an envelope containing a message and its resolver.
    pub fn new(msg: M, request: M::Resolver) -> Self {
        Self { msg, request }
    }

    pub fn new_pair(msg: M) -> (Self, M::Receipt) {
        let (resolver, receipt) = <M::Resolver as Resolver>::new();
        (Self::new(msg, resolver), receipt)
    }

    pub fn reply<T>(self, reply: T) -> Result<(), ResolveError<T>>
    where
        M: Message<Resolver = Request<T>>,
    {
        self.request.reply(reply)
    }
}

/// A type-erased [`Envelope`] used for dynamic sending.
#[derive(Debug)]
pub struct AnyEnvelope(Box<dyn Any + Send>);

impl AnyEnvelope {
    /// Create a new `AnyEnvelope` from an [`Envelope`].
    pub fn new<M: Message>(envelope: Envelope<M>) -> Self {
        Self(Box::new(envelope))
    }

    pub fn new_pair<M: Message>(msg: M) -> (Self, M::Receipt) {
        let (envelope, receipt) = Envelope::new_pair(msg);
        (Self::new(envelope), receipt)
    }

    /// Downcast the to a specific [`Envelope`].
    pub fn downcast<M: Message>(self) -> Result<Envelope<M>, Self> {
        match self.0.downcast() {
            Ok(cast) => Ok(*cast),
            Err(boxed) => Err(Self(boxed)),
        }
    }
}
