use crate::{new_request, prelude::*};
use std::{convert::Infallible, fmt::Debug};

use crate::RxError;

/// Defines how a message is sent and what kind of reply is expected.
///
/// There are two [modes](`Mode`) for the message that can be specified:
/// 1. [`FireAndForget`] mode: The sender does not expect a reply. Therefore
/// the [`Receipt`] == [`Message::Outcome`].
/// 2. [`Request`] mode: The sender expects a reply. Therefore the [`Receipt`]
/// == [`Rx<Self::Outcome>`](Rx), which resolves to the [`Message::Outcome`].
///
/// In either case, the outcome is the value that is received when the receipt is resolved.
///
/// Derive this trait using the [`derive@Message`] macro.
pub trait Message: Send + 'static + Sized {
    /// The value produced when the message is resolved.
    type Outcome: Send + 'static;

    /// The [`Mode`] of the message.
    type Mode: Mode<Self::Outcome>;
}

/// Defines mode of a [`Message`], either [`FireAndForget`] or [`Request`].
pub trait Mode<O>: sealed::Sealed {
    /// The receipt associated with the output of a message.
    type Receipt: Receipt<O>;

    /// The resolver associated with the output of a message.
    type Resolver: Debug + Send + 'static;

    /// Creates a new resolver/receipt pair.
    fn new() -> (Self::Resolver, Self::Receipt);
}

/// A handle for observing the outcome of a [`Message`].
///
/// A receipt is held by the sender and can be used to wait for the receiver
/// to resolve the message. This trait is sealed and cannot be implemented
/// outside of this crate.
pub trait Receipt<O>: Debug + Send + Sized {
    /// Waits for the message's outcome.
    fn wait(self) -> impl Future<Output = Result<O, RxError>> + Send;

    /// Waits for the message's outcome, blocking the current thread.
    fn wait_blocking(self) -> Result<O, RxError> {
        futures::executor::block_on(self.wait())
    }
}

/// The resolver associated with a [`Message`].
pub type MessageResolver<M> = <<M as Message>::Mode as Mode<<M as Message>::Outcome>>::Resolver;

/// The [`Receipt`] associated with a [`Message`].
pub type MessageReceipt<M> = <<M as Message>::Mode as Mode<<M as Message>::Outcome>>::Receipt;

/// A message mode where the sender does not expect an outcome.
///
/// The receipt and resolver are both `()`, and no value is sent between
/// the sender and receiver.
pub struct FireAndForget;

impl<O: Default> Mode<O> for FireAndForget {
    type Receipt = ();
    type Resolver = ();

    fn new() -> (Self::Resolver, Self::Receipt) {
        ((), ())
    }
}

impl<O: Default> Receipt<O> for () {
    async fn wait(self) -> Result<O, RxError> {
        Ok(O::default())
    }
}

/// A message mode where the sender waits for an outcome.
///
/// The receiver is given a [`Tx<O>`] resolver and the sender receives
/// an [`Rx<O>`] receipt.
pub struct Request;

impl<O: Send + 'static> Mode<O> for Request {
    type Receipt = Rx<O>;
    type Resolver = Tx<O>;

    fn new() -> (Self::Resolver, Self::Receipt) {
        let (tx, rx) = new_request();

        (tx, rx)
    }
}

impl<O: Send + 'static> Receipt<O> for Rx<O> {
    async fn wait(self) -> Result<O, RxError> {
        self.await
    }
}

mod sealed {
    pub trait Sealed {}

    impl Sealed for super::FireAndForget {}
    impl Sealed for super::Request {}
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
                type Mode = FireAndForget;
                type Outcome = ();
            }
        )*
    };
}
implement_message_for_base_types! {
    u8, u16, u32, u64, u128,
    i8, i16, i32, i64, i128,
    (), Infallible,
    String, &'static str
}

macro_rules! implement_message_for_wrappers {
    ($(
        $wrapper:ty
        $(where $_:ty: $where:ident)*
    ,)*) => {
        $(
            impl<M> Message for $wrapper
                where M: Send + 'static + $($where +)*
            {
                type Mode = FireAndForget;
                type Outcome = ();
            }
        )*
    };
}
implement_message_for_wrappers!(
    Box<M>,
    std::sync::Arc<M> where M: Sync,
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
                $($id: Message + Send + 'static,)*
            {
                type Mode = FireAndForget;
                type Outcome = ();
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
