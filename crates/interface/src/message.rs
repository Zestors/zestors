use crate::ResponseError;
use crate::prelude::*;
use std::{convert::Infallible, fmt::Debug};

pub trait Message: Send + 'static + Sized {
    type Receipt: Receipt<Output = Self::Output, Resolver = Self::Resolver>;

    type Resolver: Resolver<Receipt = Self::Receipt>;

    type Output: Send + 'static;
}

pub trait Receipt: Debug + Send + Sized {
    type Output: Send + 'static;
    type Resolver: Resolver;

    /// Waits for the message's outcome.
    fn wait(self) -> impl Future<Output = Result<Self::Output, ResponseError>> + Send;

    /// Waits for the message's outcome, blocking the current thread.
    fn wait_blocking(self) -> Result<Self::Output, ResponseError> {
        futures::executor::block_on(self.wait())
    }
}

pub trait Resolver: Debug + Send + Sized {
    type Receipt: Receipt;

    fn new() -> (Self, Self::Receipt);
}

impl Receipt for () {
    type Output = ();
    type Resolver = ();

    async fn wait(self) -> Result<(), ResponseError> {
        Ok(())
    }
}

impl Resolver for () {
    type Receipt = ();

    fn new() -> (Self, Self::Receipt) {
        ((), ())
    }
}

impl<T: Send + 'static> Receipt for Response<T> {
    type Output = T;
    type Resolver = Request<T>;

    async fn wait(self) -> Result<Self::Output, ResponseError> {
        self.await
    }
}

impl<T: Send + 'static> Resolver for Request<T> {
    type Receipt = Response<T>;

    fn new() -> (Self, Self::Receipt) {
        Self::new()
    }
}

/// The resolver associated with a [`Message`].
pub type MessageResolver<M> = <M as Message>::Resolver;

/// The [`Receipt`] associated with a [`Message`].
pub type MessageReceipt<M> = <M as Message>::Receipt;

// mod sealed {
//     pub trait Sealed {}

//     impl Sealed for super::FireAndForget {}
//     impl Sealed for super::Request {}
// }

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
