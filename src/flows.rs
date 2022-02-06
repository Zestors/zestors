use std::{
    convert::Infallible,
    ops::{ControlFlow, FromResidual, Try},
};

use crate::{
    action::{After, Before},
    actor::Actor,
};

/// The flow for handling a request. Similarly to how a lot of functions return a `Result<T, E>`,
/// when using `Zestors`, `Actor` functions instead return `ReqFlow<Self, R>` or `MsgFlow<Self>` or `Flow<Self>`.
///
/// A `ReqFlow` indicates that a [Reply] will be sent back to the caller. The first generic of `ReqFlow`
/// should always be `Self`, and the second parameter can be return type of this [Req].
///
/// **Errors for `ReqFlow` can be propagated in 2 ways:**
/// 1) By applying the `?`-operator directly. This propagates the error directly to the return type of
///     the request. If for example your full type is `ReqFlow<Self, std::io::Error>`, then applying the
///     `?`-operator propagates `std::io`-errors directly to the caller.
/// 2) By applying `into_exit()?`. This propagates the error directly into an `ErrorExit(E)`, which can
///     subsequently be handled by the `exiting()` callback. Errors that can be propagated here must be
///     `Into<Self::ErrorExit>`.
#[derive(Debug)]
pub enum ReqFlow<A: Actor + ?Sized, R> {
    /// Send back a reply to the caller.
    Reply(R),
    /// Send back a reply to the caller. After sending this reply, execute `After`.
    ReplyAndAfter(R, After<A>),
    /// Send back a reply to the caller. Before handling the next message, execute `Before`.
    ReplyAndBefore(R, Before<A>),
    /// Send back a reply to the caller, and then exit with an error.
    ReplyAndErrorExit(R, A::ExitError),
    /// Dont send a reply to the caller, and then exit with an error.
    IgnoreAndErrorExit(A::ExitError),
    /// Send back a reply to the caller, and then exit normally.
    ReplyAndNormalExit(R, A::ExitNormal),
    /// Dont send a reply to the caller, and then exit normally.
    IgnoreAndNormalExit(A::ExitNormal),
}

/// The flow for handling a msg. Similarly to how a lot of functions return a `Result<T, E>`,
/// when using `Zestors`, `Actor` functions instead return `ReqFlow<Self, R>` or `MsgFlow<Self>` or `Flow<Self>`.
///
/// A `MsgFlow` indicates that no [Reply] will be sent back to the caller. The generic of `MsgFlow`
/// should always be `Self`.
///
/// **Errors for `MsgFlow` can be propagated in 1 way:**
/// 1) By applying `into_exit()?`. This propagates the error directly into an `ErrorExit(E)`, which can
///     subsequently be handled by the `exiting()` callback. Errors that can be propagated here must be
///     `Into<Self::ErrorExit>`.
#[derive(Debug)]
pub enum MsgFlow<A: Actor + ?Sized> {
    /// Ok.
    Ok,
    /// Before handling the next message, execute `Before`.
    Before(Before<A>),
    /// Exit with an error.
    ErrorExit(A::ExitError),
    /// Exit normally.
    NormalExit(A::ExitNormal),
}

/// The flow for handling other operations within an actor. Similarly to how a lot of functions return a `Result<T, E>`,
/// when using `Zestors`, `Actor` functions instead return `ReqFlow<Self, R>` or `MsgFlow<Self> or `Flow<Self>`.
///
/// This `Flow`-type can NOT be used as the return type of functions, but is used for other callbacks within Zestors.
///
/// **Errors for `Flow` can be propagated in 1 way:**
/// 1) By applying `into_exit()?`. This propagates the error directly into an `ErrorExit(E)`, which can
///     subsequently be handled by the `exiting()` callback. Errors that can be propagated here must be
///     `Into<Self::ErrorExit>`.
#[derive(Debug)]
pub enum Flow<A: Actor + ?Sized> {
    /// Ok.
    Ok,
    /// Exit with an error.
    ErrorExit(A::ExitError),
    /// Exit normally.
    NormalExit(A::ExitNormal),
}


/// Applying the `?`-operating will convert this error into an actor exit.
#[must_use = "This error must be used!"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntoExit<T, E> {
    Continue(T),
    Stop(E),
}

/// A trait for converting results to `IntoExit`
pub trait Exitable<T, E> {
    /// Applying the `?`-operating will convert this error into an actor exit.
    fn into_exit(self) -> IntoExit<T, E>;
}

impl<T, E> Exitable<T, E> for Result<T, E> {
    /// Applying the `?`-operating will convert this error into an actor exit.
    fn into_exit(self) -> IntoExit<T, E> {
        match self {
            Ok(t) => IntoExit::Continue(t),
            Err(e) => IntoExit::Stop(e),
        }
    }
}

/// This is an internally used flow. All other flows are converted into this one, 
/// so that the actor can easily handle them
pub(crate) enum InternalFlow<A: Actor + ?Sized> {
    Ok,
    After(After<A>),
    Before(Before<A>),
    ErrorExit(A::ExitError),
    NormalExit(A::ExitNormal),
}

impl<A: Actor, R> ReqFlow<A, R> {
    pub(crate) fn take_reply(self) -> (InternalFlow<A>, Option<R>) {
        match self {
            ReqFlow::Reply(reply) => (InternalFlow::Ok, Some(reply)),
            ReqFlow::ReplyAndAfter(reply, handler) => (InternalFlow::After(handler), Some(reply)),
            ReqFlow::ReplyAndBefore(reply, handler) => (InternalFlow::Before(handler), Some(reply)),
            ReqFlow::ReplyAndErrorExit(reply, error) => {
                (InternalFlow::ErrorExit(error), Some(reply))
            }
            ReqFlow::IgnoreAndErrorExit(error) => (InternalFlow::ErrorExit(error), None),
            ReqFlow::ReplyAndNormalExit(reply, normal) => {
                (InternalFlow::NormalExit(normal), Some(reply))
            }
            ReqFlow::IgnoreAndNormalExit(normal) => (InternalFlow::NormalExit(normal), None),
        }
    }
}

impl<A: Actor> MsgFlow<A> {
    pub(crate) fn into_internal(self) -> InternalFlow<A> {
        match self {
            MsgFlow::Ok => InternalFlow::Ok,
            MsgFlow::Before(handler) => InternalFlow::Before(handler),
            MsgFlow::ErrorExit(error) => InternalFlow::ErrorExit(error),
            MsgFlow::NormalExit(normal) => InternalFlow::NormalExit(normal),
        }
    }
}

impl<A: Actor> Flow<A> {
    pub(crate) fn into_internal(self) -> InternalFlow<A> {
        match self {
            Flow::Ok => InternalFlow::Ok,
            Flow::ErrorExit(error) => InternalFlow::ErrorExit(error),
            Flow::NormalExit(normal) => InternalFlow::NormalExit(normal),
        }
    }
}

unsafe impl<A: Actor> Send for MsgFlow<A> {}
unsafe impl<A: Actor> Send for Flow<A> {}
unsafe impl<A: Actor, R> Send for ReqFlow<A, R> {}
unsafe impl<A: Actor> Send for InternalFlow<A> {}


//-------------------------------------
// Try implementations
//-------------------------------------

impl<T, E> Try for IntoExit<T, E> {
    type Output = T;

    type Residual = IntoExit<Infallible, E>;

    fn from_output(output: Self::Output) -> Self {
        Self::Continue(output)
    }

    fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
        match self {
            IntoExit::Continue(t) => ControlFlow::Continue(t),
            IntoExit::Stop(e) => ControlFlow::Break(IntoExit::Stop(e)),
        }
    }
}

impl<T, E> FromResidual<IntoExit<Infallible, E>> for IntoExit<T, E> {
    fn from_residual(residual: IntoExit<Infallible, E>) -> Self {
        match residual {
            IntoExit::Continue(_inf) => unreachable!(),
            IntoExit::Stop(e) => Self::Stop(e),
        }
    }
}

//-------------------------------------
// FromResidual<IntoStop>
//-------------------------------------

impl<A: Actor, R, E> FromResidual<IntoExit<Infallible, E>> for ReqFlow<A, R>
where
    E: Into<A::ExitError>,
{
    fn from_residual(residual: IntoExit<Infallible, E>) -> Self {
        match residual {
            IntoExit::Continue(_inf) => unreachable!(),
            IntoExit::Stop(e) => ReqFlow::IgnoreAndErrorExit(e.into()),
        }
    }
}

impl<A: Actor, E> FromResidual<IntoExit<Infallible, E>> for MsgFlow<A>
where
    E: Into<A::ExitError>,
{
    fn from_residual(residual: IntoExit<Infallible, E>) -> Self {
        match residual {
            IntoExit::Continue(_inf) => unreachable!(),
            IntoExit::Stop(e) => MsgFlow::ErrorExit(e.into()),
        }
    }
}

impl<A: Actor, E> FromResidual<IntoExit<Infallible, E>> for Flow<A>
where
    E: Into<A::ExitError>,
{
    fn from_residual(residual: IntoExit<Infallible, E>) -> Self {
        match residual {
            IntoExit::Continue(_inf) => unreachable!(),
            IntoExit::Stop(e) => Flow::ErrorExit(e.into()),
        }
    }
}

//-------------------------------------
// FromResidual<Result>
//-------------------------------------

impl<A: Actor, E, F, T> FromResidual<Result<Infallible, E>> for ReqFlow<A, Result<T, F>>
where
    F: From<E>,
{
    fn from_residual(residual: Result<Infallible, E>) -> Self {
        match residual {
            Ok(_inf) => unreachable!(),
            Err(e) => ReqFlow::Reply(Err(e.into())),
        }
    }
}
