use crate::*;
use std::any::TypeId;

//------------------------------------------------------------------------------------------------
//  Protocol
//------------------------------------------------------------------------------------------------

/// In order for an actor to receive any messages, it must first define a [Protocol].
/// This protocol defines exactly which [Messages](Message) the actor can receive, and how
/// to convert these messages into the [Protocol] that is received by the [Inbox].
///
/// This is normally derived using the [protocol] macro, but may be implemented manually.
pub trait Protocol: Send + 'static {
    /// Attempt to create the [Protocol] from a [BoxedMessage].
    ///
    /// This should return `Ok(Self)`, if it is downcast-able to an accepted message.
    /// Otherwise this should return `Err(BoxedMessage)`.
    fn try_from_boxed(boxed: BoxedMessage) -> Result<Self, BoxedMessage>
    where
        Self: Sized;

    /// Convert the [Protocol] into a [BoxedMessage].
    ///
    /// This should be implemented by matching on the message type creating a [BoxedMessage]
    /// from that.
    fn into_boxed(self) -> BoxedMessage;

    /// Whether the [Protocol] accepts a message.
    ///
    /// The type-id given here is the `TypeId` of the message, and should return true
    /// if this message is accepted.
    fn accepts(id: &TypeId) -> bool
    where
        Self: Sized;
}

//------------------------------------------------------------------------------------------------
//  ProtocolMessage
//------------------------------------------------------------------------------------------------

/// In order to send message `M` to an actor, it's protocol must implement `ProtocolMessage<M>`.
///
/// This is normally derived using the [protocol] macro, but may be implemented manually.
pub trait ProtocolMessage<M: Message>: Protocol {
    /// Create the [Protocol] from `Sends<M>`.
    fn from_sends(msg: Sends<M>) -> Self
    where
        Self: Sized;

    /// Attempt to extract message `M` from the [Protocol].
    ///
    /// This should return the `Ok(Sends<M>)` if the [Protocol] is of that type, otherwise
    /// this should return `Err(Self)`.
    fn try_into_sends(self) -> Result<Sends<M>, Self>
    where
        Self: Sized;
}

//------------------------------------------------------------------------------------------------
//  ProtocolMessageExt
//------------------------------------------------------------------------------------------------

/// A helper trait
pub(crate) trait ProtocolMessageExt<M: Message>: ProtocolMessage<M> {
    fn unwrap_into_msg(self, returns: Returns<M>) -> M;
}

impl<M: Message, T> ProtocolMessageExt<M> for T
where
    T: ProtocolMessage<M>,
{
    fn unwrap_into_msg(self, returns: Returns<M>) -> M {
        match self.try_into_sends() {
            Ok(sends) => <M::Type as MsgType<M>>::into_msg(sends, returns),
            Err(_) => panic!(),
        }
    }
}

//------------------------------------------------------------------------------------------------
//  Message
//------------------------------------------------------------------------------------------------

/// To use a message withing `zestors`, it must implement [Message].
///
/// This is normally derived using [Message](crate::Message).
/// ```
pub trait Message: Sized {
    type Type: MsgType<Self>;
}

//------------------------------------------------------------------------------------------------
//  MsgType
//------------------------------------------------------------------------------------------------

/// Every message has a type: It's `MsgType`, which decides what kind of message it is.
///
/// Normally, it is not necessary to implement this, but it may be used for custom message-types.
pub trait MsgType<M> {
    /// This is the message that is actually sent to the actor.
    ///
    /// It is called the sender-part of the message, or `Sends<M>`.
    type Sends;

    /// This is that which is returned when a message is sent to an actor.
    ///
    /// It is called the returns-part of the message, or `Returns<M>`.
    type Returns;

    /// Create the send-return pair from the message.
    fn new_pair(msg: M) -> (Self::Sends, Self::Returns);

    /// Turn the pair back into the message.
    fn into_msg(sends: Self::Sends, returns: Self::Returns) -> M;
}

impl<M> MsgType<M> for () {
    type Sends = M;
    type Returns = ();

    fn new_pair(msg: M) -> (M, ()) {
        (msg, ())
    }

    fn into_msg(sends: M, _returns: ()) -> M {
        sends
    }
}

//------------------------------------------------------------------------------------------------
//  Sends + Returns
//------------------------------------------------------------------------------------------------

/// A shorthand for writing `<<M as Message>::Type as MsgType<M>>::Sends`, the
/// sender-part of a message.
pub type Sends<M> = <<M as Message>::Type as MsgType<M>>::Sends;

/// A shorthand for writing `<<M as Message>::Type as MsgType<M>>::Returns`, the
/// returner-part of a message.
pub type Returns<M> = <<M as Message>::Type as MsgType<M>>::Returns;

mod default_messages {
    use crate::*;
    use std::{borrow::Cow, rc::Rc, sync::Arc};

    macro_rules! default_messages {
        ($(
            $ty:ty
        ),*) => {
            $(
                impl Message for $ty {
                    type Type = ();
                }
            )*
        };
    }

    default_messages! {
        u8, u16, u32, u64, u128,
        i8, i16, i32, i64, i128,
        ()
    }

    macro_rules! default_message_tuples {
        ($(
            ($($id:ident),*),
        )*) => {
            $(
                impl<$($id),*> Message for ($($id,)*)
                where
                    $($id: Message<Type = ()>,)*
                {
                    type Type = ();
                }
            )*
        };
    }

    default_message_tuples!(
        (M1),
        (M1, M2),
        (M1, M2, M3),
        (M1, M2, M3, M4),
        (M1, M2, M3, M4, M5),
        (M1, M2, M3, M4, M5, M6),
        (M1, M2, M3, M4, M5, M6, M7),
        (M1, M2, M3, M4, M5, M6, M7, M8),
        (M1, M2, M3, M4, M5, M6, M7, M8, M9),
        (M1, M2, M3, M4, M5, M6, M7, M8, M9, M10),
    );

    macro_rules! default_message_wrappers {
        ($(
            $(:$lf:lifetime)?
            $wrapper:ty
            $(where $_:ty: $where:ident)*
        ,)*) => {
            $(
                impl<$($lf,)? M> Message for $wrapper
                    where M: Message<Type = ()> + $($where +)*
                {
                    type Type = ();
                }
            )*
        };
    }

    default_message_wrappers!(
        Box<M>,
        Arc<M>,
        Rc<M>,
        Vec<M>,
        Box<[M]>,
        :'a Cow<'a, M> where M: Clone,
    );
}
