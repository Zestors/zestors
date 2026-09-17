use crate::{ReceiptError, message::sealed::Sealed};
use crate::{ResolveError, prelude::*};
use std::{convert::Infallible, fmt::Debug};

/// Defines whether a message is a fire-and-forget or request-style message.
///
/// When the message is sent, an [`Envelope`] is constructed that contains both the
/// message itself, as well as the associated [`Resolver`]. The sender immeadeately
/// receives the [`Receipt`] of sending the message.
///
/// Receipts and resolvers come in two types:
/// - `fire-and-forget`: Both the resolver and the request are of type `()`. No reply
/// is expected.
/// - `request`: The resolver is a [`Request<T>`], and the receipt is a
/// [`Response<T>`]. Once a reply is sent, the response resolves to `T`.
///
/// This trait must be implemented for any message that is sent in zestors.
/// It can easily be [derived](derive@Message) as well.
pub trait Message: Send + 'static + Sized {
    /// The receipt associated with this message, that is returned after sending.
    type Receipt: Receipt<Output = Self::Output, Resolver = Self::Resolver>;

    /// The resolver used to resolve the receipt given to the sender.
    type Resolver: Resolver<Receipt = Self::Receipt>;

    /// The output of the receipt after being resolved.
    type Output: Send + 'static;
}

/// The the value returned after sending a [`Message`].
pub trait Receipt: Debug + Send + Sized + Sealed {
    /// The output of the receipt after being resolved.
    type Output: Send + 'static;

    /// The resolver used to resolve this receipt.
    type Resolver: Resolver;

    /// Waits for the message's outcome.
    fn wait(self) -> impl Future<Output = Result<Self::Output, ReceiptError>> + Send;

    /// Waits for the message's outcome, blocking the current thread.
    fn wait_blocking(self) -> Result<Self::Output, ReceiptError> {
        futures::executor::block_on(self.wait())
    }
}

/// The value passed along with a [`Message`], used to resolve a [`Receipt`]
pub trait Resolver: Debug + Send + Sized + Sealed {
    /// The receipt associated with this resolver.
    type Receipt: Receipt;

    /// The input type used to resolve this receipt.
    type Input;

    /// Construct a new resolver-receipt pair
    fn new() -> (Self, Self::Receipt);

    /// Resolves the receipt with the given input, returning an error if the resolution fails.
    fn resolve(self, input: Self::Input) -> Result<(), ResolveError<Self::Input>>;
}

impl Receipt for () {
    type Output = ();
    type Resolver = ();

    async fn wait(self) -> Result<(), ReceiptError> {
        Ok(())
    }
}

impl Resolver for () {
    type Receipt = ();

    type Input = ();

    fn new() -> (Self, Self::Receipt) {
        ((), ())
    }

    fn resolve(self, _: Self::Input) -> Result<(), ResolveError<Self::Input>> {
        Ok(())
    }
}

impl<T: Send + 'static> Resolver for Request<T> {
    type Receipt = Reply<T>;

    type Input = T;

    fn new() -> (Self, Self::Receipt) {
        Self::new()
    }

    fn resolve(self, input: Self::Input) -> Result<(), ResolveError<Self::Input>> {
        self.reply(input)
    }
}

impl<T: Send + 'static> Receipt for Reply<T> {
    type Output = T;
    type Resolver = Request<T>;

    async fn wait(self) -> Result<Self::Output, ReceiptError> {
        self.await
    }
}

mod sealed {
    pub trait Sealed {}

    impl Sealed for () {}
    impl<T> Sealed for super::Request<T> {}
    impl<T> Sealed for super::Reply<T> {}
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
                type Output = ();
                type Receipt = ();
                type Resolver = ();
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
                type Output = ();
                type Receipt = ();
                type Resolver = ();
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
                type Output = ();
                type Receipt = ();
                type Resolver = ();
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
