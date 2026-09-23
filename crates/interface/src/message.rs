use crate::{ReceiptError, message::sealed::Sealed};
use crate::{ResolveError, prelude::*};
use std::{convert::Infallible, fmt::Debug};

/// A type that can be sent to an actor.
///
/// A message is either fire-and-forget or request/reply, decided by its
/// [`Kind`](Message::Kind):
/// - [`Cast`]: no reply is expected. [`Output`](Message::Output) is `()`, and
///   the message travels without a reply channel.
/// - [`Call`]: the actor replies with an `Output`. The message travels with a
///   [`Request<Output>`], and the sender waits on the matching
///   [`Reply<Output>`].
///
/// Implement it with [`#[derive(Message)]`](derive@Message), adding
/// `#[msg(reply = T)]` for a request/reply message. A few common types
/// (integers, `String`, `()`, …) are fire-and-forget messages already.
///
/// # Example
///
/// ```
/// # use zestors::interface::Message;
/// // Fire-and-forget: `Output = ()`, `Kind = Cast`.
/// #[derive(Message, Debug)]
/// # #[zestors(interface_path = "zestors::interface")]
/// struct Greet(String);
///
/// // Request/reply: `Output = u32`, `Kind = Call`.
/// #[derive(Message, Debug)]
/// #[msg(reply = u32)]
/// # #[zestors(interface_path = "zestors::interface")]
/// struct CountLetters(String);
/// ```
pub trait Message: Send + 'static + Sized {
    /// What the sender gets back: the reply for a [`Call`] message, `()` for a
    /// [`Cast`] message.
    type Output: Send + 'static;

    /// Whether the message expects a reply: [`Call`] or [`Cast`].
    type Kind: MessageKind<Self::Output>;
}

/// The two kinds of message, [`Call`] and [`Cast`], and the reply channel
/// each one travels with.
pub trait MessageKind<O> {
    /// What the sender waits on: [`Reply<O>`] or `()`.
    type Receipt: Receipt<Output = O, Resolver = Self::Resolver>;
    /// What the receiver answers with: [`Request<O>`] or `()`.
    type Resolver: Resolver<Input = O, Receipt = Self::Receipt>;
}

/// The request/reply [`MessageKind`]: the actor answers with the message's
/// [`Output`](Message::Output).
pub struct Call;
/// The fire-and-forget [`MessageKind`]: nothing is sent back.
pub struct Cast;

impl<O: Send + 'static> MessageKind<O> for Call {
    type Receipt = Reply<O>;
    type Resolver = Request<O>;
}
impl MessageKind<()> for Cast {
    type Receipt = ();
    type Resolver = ();
}

/// The reply channel a message `M` travels with: [`Request<M::Output>`](Request)
/// for a [`Call`] message, `()` for a [`Cast`] message. This is the type of the
/// `req` argument in a handler.
pub type ResolverOf<M> = <<M as Message>::Kind as MessageKind<<M as Message>::Output>>::Resolver;
/// What the sender of a message `M` gets back to wait on:
/// [`Reply<M::Output>`](Reply) for a [`Call`] message, `()` for a [`Cast`]
/// message.
pub type ReceiptOf<M> = <<M as Message>::Kind as MessageKind<<M as Message>::Output>>::Receipt;

/// The value returned to the sender after sending a [`Message`]; can be
/// awaited (or, for [`Reply`], blocked on) to obtain the message's outcome.
///
/// A fire-and-forget message's receipt is `()`, which resolves immediately
/// to `Ok(())`; see [`Message`]'s example for the request-style case, where
/// the receipt is a [`Reply<T>`] instead.
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

/// The value passed along with a [`Message`], used to resolve a [`Receipt`].
pub trait Resolver: Debug + Send + Sized + Sealed {
    /// The receipt associated with this resolver.
    type Receipt: Receipt;

    /// The input type used to resolve this receipt.
    type Input;

    /// Constructs a new resolver/receipt pair.
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

    // Overrides the default (`futures::executor::block_on(self.wait())`) to
    // go through tokio's dedicated blocking-recv instead.
    fn wait_blocking(self) -> Result<Self::Output, ReceiptError> {
        Reply::wait_blocking(self)
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
// `Message` is implemented directly (fire-and-forget: `Output = ()`,
// `Kind = Cast`) for a handful of common types below, so they can be used
// as trivial notifications - e.g. `actor.cast(42u32)` - without needing a
// `#[derive(Message)]` wrapper type. This does *not* extend to `Interface`:
// an actor still only accepts these types if its `Interface` has a variant
// for them.

macro_rules! implement_message_for_base_types {
    ($(
        $ty:ty
    ),*) => {
        $(
            impl Message for $ty {
                type Output = ();
                type Kind = Cast;
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
                type Kind = Cast;
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
                type Kind = Cast;
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
