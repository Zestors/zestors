use super::*;
use std::any::Any;

/// A wrapper-type around a `Box<dyn Any + Send>` for a [`Message::Payload`].
#[derive(Debug)]
pub struct BoxPayload(Box<dyn Any + Send>);

impl BoxPayload {
    /// Create a new [`BoxPayload`] from the [`Message::Payload`].
    pub fn new<M>(sent: M::Payload) -> Self
    where
        M: Message,
        M::Payload: Send + 'static,
    {
        Self(Box::new(sent))
    }

    /// Downcast the [`BoxPayload`] into a [`Message::Payload`].
    pub fn downcast<M>(self) -> Result<M::Payload, Self>
    where
        M: Message,
        M::Payload: 'static,
    {
        match self.0.downcast() {
            Ok(cast) => Ok(*cast),
            Err(boxed) => Err(Self(boxed)),
        }
    }

    pub(crate) fn downcast_and_cancel<M>(self, returned: M::Returned) -> Result<M, Self>
    where
        M: Message,
        M::Payload: 'static,
    {
        match self.downcast::<M>() {
            Ok(sends) => Ok(M::cancel(sends, returned)),
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
            type Payload = Self;
            type Returned = ();
            fn create(self) -> (Self::Payload, Self::Returned) {
                (self, ())
            }
            fn cancel(sent: Self::Payload, _returned: Self::Returned) -> Self {
                sent
            }
        }

        impl Message for Msg2 {
            type Payload = Self;
            type Returned = ();
            fn create(self) -> (Self::Payload, Self::Returned) {
                (self, ())
            }
            fn cancel(sent: Self::Payload, _returned: Self::Returned) -> Self {
                sent
            }
        }

        let boxed = BoxPayload::new::<Msg1>(Msg1);
        assert!(boxed.downcast::<Msg1>().is_ok());

        let boxed = BoxPayload::new::<Msg1>(Msg1);
        assert!(boxed.downcast::<Msg2>().is_err());
    }
}
