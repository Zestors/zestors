use super::*;
use std::any::TypeId;

/// The [Message] trait must be implemented for all messages you would like to send.
/// This trait defines of what [MessageType] the message is. By using a different
/// [MessageType], behaviour of sending can be changed. (For example to require a
/// reply)
///
/// This trait can be derived using [`#[derive(Debug)]`](crate::Message!).
pub trait Message: Sized {
    /// The type of this message.
    type Type: MessageType<Self>;
}

/// The [MessageType] trait is implemented for custom message types. The [MessageType] decides
/// what happens when a message is created/sent, and when it is canceled. For most uses,
/// this does not have to be implemented manually, but one of the following can be used:
/// - `()`: For sending one-off messages.
/// - `Rx<T>`: For messages with reply of type `T`.
pub trait MessageType<M> {
    /// The message that is sent.
    type Sent;

    /// The value that is returned when a message is sent.
    type Returned;

    /// This is called before the message is sent.
    fn create(msg: M) -> (Self::Sent, Self::Returned);

    /// This is called if the message cannot be sent succesfully.
    fn cancel(sent: Self::Sent, returned: Self::Returned) -> M;
}

/// Every actor has to define a [Protocol], which defines the messages that it
/// accepts by implementing [ProtocolMessage<M>] for those messages.
///
/// This can be derived with [`#derive[protocol]`](crate::protocol!).
pub trait Protocol: Send + 'static + Sized {
    /// Convert the protocol into a [BoxedMessage].
    fn into_box(self) -> BoxedMessage;

    /// Attempt to convert a [BoxedMessage] into the [Protocol].
    ///
    /// This should succeed if the [Protocol] accepts a message, otherwise this
    /// should fail.
    fn try_from_box(boxed: BoxedMessage) -> Result<Self, BoxedMessage>;

    /// Whether the [Protocol] accepts a message. The `TypeId` is that of the
    /// message.
    ///
    /// This should succeed if the [Protocol] accepts a message, otherwise this
    /// should fail.
    fn accepts_msg(msg_id: &TypeId) -> bool;
}

/// The trait [ProtocolMessage<M>] should be implemented for all messages `M` that a
/// [Protocol] accepts.
///
/// This can be derived with [`#derive[protocol]`](crate::protocol!).
pub trait ProtocolMessage<M: Message> {
    /// Convert the [Msg] into the [Protocol].
    fn from_msg(msg: Sent<M>) -> Self
    where
        Self: Sized;

    /// Attempt to convert the [Protocol] into a specific [Message] if it is of that
    /// type.
    fn try_into_msg(self) -> Result<Sent<M>, Self>
    where
        Self: Sized;

    /// Automatically implemented method to unwrap and cancel a message.
    fn unwrap_and_cancel(self, returned: Returned<M>) -> M
    where
        Self: Sized,
    {
        if let Ok(sent) = self.try_into_msg() {
            <M::Type as MessageType<M>>::cancel(sent, returned)
        } else {
            panic!("")
        }
    }
}

/// A shorthand for writing [<M::Type as MessageType<M>>::Sent](MessageType).
pub type Sent<M> = <<M as Message>::Type as MessageType<M>>::Sent;

/// A shorthand for writing [<M::Type as MessageType<M>>::Returned](MessageType).
pub type Returned<M> = <<M as Message>::Type as MessageType<M>>::Returned;

mod test {
    use super::*;

    #[allow(unused)]
    type TestDyn1 = Box<dyn ProtocolMessage<u32>>;
    #[allow(unused)]
    type TestDyn2 = Box<dyn Protocol>;
}
