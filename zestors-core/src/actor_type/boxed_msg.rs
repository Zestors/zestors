use super::*;
use std::any::Any;

/// A simple wrapper around a `Box<dyn Any>` that only allows messages to be created
/// from [Sent<M>] if `M` implements [Message].
#[derive(Debug)]
pub struct BoxedMessage(Box<dyn Any + Send>);

impl BoxedMessage {
    /// Create a new [BoxedMessage] from [Sent<M>].
    pub fn new<M>(sends: Sent<M>) -> Self
    where
        M: Message,
        Sent<M>: Send + 'static,
    {
        Self(Box::new(sends))
    }

    /// Downcast the [BoxedMessage] into [Sent<M>].
    pub fn downcast<M>(self) -> Result<Sent<M>, Self>
    where
        M: Message,
        Sent<M>: 'static,
    {
        match self.0.downcast() {
            Ok(cast) => Ok(*cast),
            Err(boxed) => Err(Self(boxed)),
        }
    }

    pub(crate) fn downcast_cancel<M>(self, returned: Returned<M>) -> Result<M, Self>
    where
        M: Message,
        Sent<M>: 'static,
    {
        match self.downcast::<M>() {
            Ok(sends) => Ok(<M::Type as MessageType<M>>::cancel(sends, returned)),
            Err(boxed) => Err(boxed),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn boxed_msg() {
        struct Msg1;
        struct Msg2;

        impl Message for Msg1 {
            type Type = ();
        }

        impl Message for Msg2 {
            type Type = ();
        }

        let boxed = BoxedMessage::new::<Msg1>(Msg1);
        assert!(boxed.downcast::<Msg1>().is_ok());

        let boxed = BoxedMessage::new::<Msg1>(Msg1);
        assert!(boxed.downcast::<Msg2>().is_err());
    }
}
