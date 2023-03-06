#[allow(unused)]
use crate::all::*;
use std::sync::Arc;

/// Any message should implement [`Message`], and has two associated types:
/// - [`Message::Payload`] -> The payload of the message that is sent to the actor.
/// - [`Message::Returned`] -> The value that is returned when the message is sent.
///
/// A simple message has the following associated types:
/// - `Payload = Self`
/// - `Returned = ()`
///
/// A request of type `R` has the following associated types:
/// - `Payload = (Self, Tx<R>)`
/// - `Returned = Rx<R>`
///
/// # Derive
/// [`Message`] can be derived with the [`derive@Message`] macro.
pub trait Message: Sized {
    ///  The payload of the message that is sent to the actor.
    type Payload: Send + 'static;

    /// The value that is returned when the message is sent.
    type Returned;

    /// This is called before the message is sent.
    fn create(self) -> (Self::Payload, Self::Returned);

    /// This is called if the message cannot be sent.
    fn cancel(sent: Self::Payload, returned: Self::Returned) -> Self;
}

/// A version of the [`Message`] trait generic over `M`, used as the `[msg(..)]` attribute for the [`derive@Message`] derive
/// macro.
/// 
/// This is implemented for `()`, [`Rx<_>`] and [`Tx<_>`]
pub trait MessageDerive<M> {
    /// See [`Message::Payload`].
    type Payload;

    /// See [`Message::Returned`].
    type Returned;

    /// See [`Message::create`].
    fn create(msg: M) -> (Self::Payload, Self::Returned);

    /// See [`Message::cancel`].
    fn cancel(sent: Self::Payload, returned: Self::Returned) -> M;
}

impl<M: Send + 'static> MessageDerive<M> for () {
    type Payload = M;
    type Returned = ();
    fn create(msg: M) -> (M, ()) {
        (msg, ())
    }
    fn cancel(payload: M, _returns: ()) -> M {
        payload
    }
}

//------------------------------------------------------------------------------------------------
//  Message: Default implementations
//------------------------------------------------------------------------------------------------

macro_rules! implement_message_for_base_types {
    ($(
        $ty:ty
    ),*) => {
        $(
            impl Message for $ty {
                type Payload = Self;
                type Returned = ();
                fn create(self) -> (Self, ()) {
                    (self, ())
                }
                fn cancel(payload: Self, _returns: ()) -> Self {
                    payload
                }
            }
        )*
    };
}
implement_message_for_base_types! {
    u8, u16, u32, u64, u128,
    i8, i16, i32, i64, i128,
    (),
    String, &'static str
}

macro_rules! implement_message_for_wrappers {
    ($(
        $wrapper:ty
        $(where $_:ty: $where:ident)*
    ,)*) => {
        $(
            impl<M> Message for $wrapper
                where M: SimpleMessage + Send + 'static + $($where +)*
            {
                type Payload = Self;
                type Returned = ();
                fn create(self) -> (Self, ()) {
                    (self, ())
                }
                fn cancel(payload: Self, _returns: ()) -> Self {
                    payload
                }
            }
        )*
    };
}
implement_message_for_wrappers!(
    Box<M>,
    Arc<M> where M: Sync,
    Vec<M>,
    Box<[M]>,
);

macro_rules! implement_message_kind_and_message_for_tuples {
    ($(
        ($($id:ident: $na:ident + $na2:ident),*),
    )*) => {
        $(
            impl<$($id),*> Message for ($($id,)*)
            where
                $($id: SimpleMessage + Send + 'static,)*
            {
                type Payload = Self;
                type Returned = ();
                fn create(self) -> (Self, ()) {
                    (self, ())
                }
                fn cancel(payload: Self, _returns: ()) -> Self {
                    payload
                }
            }
        )*
    };
}
implement_message_kind_and_message_for_tuples!(
    (M1: m1 + m_1),
    (M1: m1 + m_1, M2: m2 + m_2),
    (M1: m1 + m_1, M2: m2 + m_2, M3: m3 + m_3),
    (M1: m1 + m_1, M2: m2 + m_2, M3: m3 + m_3, M4: m4 + m_4),
    (
        M1: m1 + m_1,
        M2: m2 + m_2,
        M3: m3 + m_3,
        M4: m4 + m_4,
        M5: m5 + m_5
    ),
    (
        M1: m1 + m_1,
        M2: m2 + m_2,
        M3: m3 + m_3,
        M4: m4 + m_4,
        M5: m5 + m_5,
        M6: m6 + m_6
    ),
    (
        M1: m1 + m_1,
        M2: m2 + m_2,
        M3: m3 + m_3,
        M4: m4 + m_4,
        M5: m5 + m_5,
        M6: m6 + m_6,
        M7: m7 + m_7
    ),
    (
        M1: m1 + m_1,
        M2: m2 + m_2,
        M3: m3 + m_3,
        M4: m4 + m_4,
        M5: m5 + m_5,
        M6: m6 + m_6,
        M7: m7 + m_7,
        M8: m8 + m_8
    ),
    (
        M1: m1 + m_1,
        M2: m2 + m_2,
        M3: m3 + m_3,
        M4: m4 + m_4,
        M5: m5 + m_5,
        M6: m6 + m_6,
        M7: m7 + m_7,
        M8: m8 + m_8,
        M9: m9 + m_9
    ),
    (
        M1: m1 + m_1,
        M2: m2 + m_2,
        M3: m3 + m_3,
        M4: m4 + m_4,
        M5: m5 + m_5,
        M6: m6 + m_6,
        M7: m7 + m_7,
        M8: m8 + m_8,
        M9: m9 + m_9,
        M10: m10 + m_10
    ),
);

trait SimpleMessage: Message<Payload = Self, Returned = ()> {}
impl<T> SimpleMessage for T where T: Message<Payload = Self, Returned = ()> {}
