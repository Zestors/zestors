use std::{borrow::Cow, rc::Rc, sync::Arc};

/// To use a message withing `zestors`, it must implement [Message].
///
/// This is normally derived using [Message](crate::Message).
/// ```
pub trait Message: Sized {
    type Type: MessageType<Self>;
}

/// Every message has a type: It's `MsgType`, which decides what kind of message it is.
///
/// Normally, it is not necessary to implement this, but it may be used for custom message-types.
pub trait MessageType<M> {
    /// This is the message that is actually sent to the actor.
    ///
    /// It is called the sender-part of the message, or `Sends<M>`.
    type Sends;

    /// This is that which is returned when a message is sent to an actor.
    ///
    /// It is called the returns-part of the message, or `Returns<M>`.
    type Returns;

    /// Create the send-return pair from the message.
    fn create(msg: M) -> (Self::Sends, Self::Returns);

    /// Turn the pair back into the message.
    fn destroy(sends: Self::Sends, returns: Self::Returns) -> M;
}

impl<M> MessageType<M> for () {
    type Sends = M;
    type Returns = ();

    fn create(msg: M) -> (M, ()) {
        (msg, ())
    }

    fn destroy(sends: M, _returns: ()) -> M {
        sends
    }
}

/// A shorthand for writing `<<M as Message>::Type as MsgType<M>>::Sends`, the
/// sender-part of a message.
pub type SendPart<M> = <<M as Message>::Type as MessageType<M>>::Sends;

/// A shorthand for writing `<<M as Message>::Type as MsgType<M>>::Returns`, the
/// returner-part of a message.
pub type ReturnPart<M> = <<M as Message>::Type as MessageType<M>>::Returns;

//------------------------------------------------------------------------------------------------
//  Default base types
//------------------------------------------------------------------------------------------------

macro_rules! default_base_types {
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

default_base_types! {
    u8, u16, u32, u64, u128,
    i8, i16, i32, i64, i128,
    (),
    String, &'static str
}

//------------------------------------------------------------------------------------------------
//  Default tuples
//------------------------------------------------------------------------------------------------

macro_rules! default_tuples {
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

default_tuples!(
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

//------------------------------------------------------------------------------------------------
//  Default wrappers
//------------------------------------------------------------------------------------------------

macro_rules! default_wrappers {
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

default_wrappers!(
    Box<M>,
    Arc<M>,
    Rc<M>,
    Vec<M>,
    Box<[M]>,
    :'a Cow<'a, M> where M: Clone,
);
