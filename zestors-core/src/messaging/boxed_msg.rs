use crate::*;
use std::any::Any;

/// A simple wrapper around a `Box<dyn Any + Send + 'static>`, which only allows
/// the `Sends<M>` to be stored inside.
#[derive(Debug)]
pub struct BoxedMessage(Box<dyn Any + Send + 'static>);

impl BoxedMessage {
    /// Create a new `BoxedMessage` from the `Sends<M>`.
    pub fn new<M>(sends: Sends<M>) -> Self
    where
        M: Message,
        Sends<M>: Send + 'static,
    {
        Self(Box::new(sends))
    }

    /// Downcast the `BoxedMessage` to the `Sends<M>`.
    pub fn downcast<M>(self) -> Result<Sends<M>, Self>
    where
        M: Message,
        Sends<M>: Send + 'static
    {
        match self.0.downcast() {
            Ok(cast) => Ok(*cast),
            Err(boxed) => Err(Self(boxed)),
        }
    }

    /// Downcast the `BoxedMessage` to the `Sends<M>`, and then get `M`.
    pub fn downcast_into_msg<M>(self, returns: Returns<M>) -> Result<M, Self>
    where
        M: Message,
        Sends<M>: Send + 'static
    {
        match self.downcast::<M>() {
            Ok(sends) => Ok(<M::Type as MessageType<M>>::into_msg(sends, returns)),
            Err(boxed) => Err(boxed),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::*;

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
