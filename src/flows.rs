use std::{
    convert::Infallible,
    ops::{ControlFlow, FromResidual, Try},
};

use crate::{action::Action, actor::Actor};

//--------------------------------------------------------------------------------------------------
//  Flow definitions
//--------------------------------------------------------------------------------------------------

/// The flow for handling a `Req`. Similarly to how a lot of functions return a `Result<T, E>`,
/// when using `Zestors`, `Actor` functions instead return `ReqFlow<Self, R>` or `MsgFlow<Self>`.
///
/// A `ReqFlow` indicates that a [Reply] will be sent back to the caller. The first generic of 
/// `ReqFlow` should always be `Self`, and the second parameter can be return type of this [Req].
///
/// **Errors for `ReqFlow` can be propagated in 2 ways:**
/// 1) By applying `.into_reply()`. This propagates the error directly to the return type of
///     the request. If for example your full type is `ReqFlow<Self, std::io::Error>`, then 
///     applying the `.into_reply()?`-operator propagates `std::io`-errors directly to the caller.
/// 2) By applying `?` operator directly. This propagates the error directly into an 
///     `ExitWithError(E)`, which can subsequently be handled by the `handle_exit()` callback. 
///     Errors that can be propagated here must implement `Into<Self::ExitError>`.
#[derive(Debug)]
pub enum ReqFlow<A: Actor + ?Sized, R> {
    /// Send back a reply to the caller.
    Reply(R),
    /// Send back a reply to the caller. Right after sending this reply, execute this `Action`.
    ReplyAndAfter(R, Action<A>),
    /// Send back a reply to the caller. 
    /// Right before handling the next message, execute this `Action`.
    ReplyAndBefore(R, Action<A>),
    /// Send back a reply to the caller, and then exit with an error.
    ReplyAndExitWithError(R, A::ExitError),
    /// Dont send a reply to the caller, and then exit with an error.
    ExitWithError(A::ExitError),
    /// Send back a reply to the caller, and then exit normally.
    ReplyAndNormalExit(R, A::ExitNormal),
    /// Dont send a reply to the caller, and then exit normally.
    NormalExit(A::ExitNormal),
}

/// The flow for handling a `Msg` or an `Action`. Similarly to how a lot of functions return a
/// `Result<T, E>`, when using `Zestors`, `Actor` functions instead return `ReqFlow<Self, R>`
/// or `MsgFlow<Self>`. The generic parameter of `MsgFlow` should always be `Self`.
///
/// Errors for `MsgFlow` can be propagated in 1 by applying `?` directly. This propagates the error
/// directly into an `ExitWithError(E)`, which can subsequently be handled by the `handle_exit()`
/// callback. Errors that can be propagated here must implement `Into<Self::ExitError>`.
///
/// ### Care!
/// If `MsgFlow::Before` is the return type of an `Action` from another `MsgFlow::Before`, then this
/// Action will be silently ignored and NOT executed!
#[derive(Debug)]
pub enum MsgFlow<A: Actor + ?Sized> {
    /// Ok.
    Ok,
    /// Right before handling the next message, execute this `Action`.
    /// ### Care!
    /// If this `Action` returns another `MsgFlow::Before`, then this `Action` will be silently ignored
    /// and NOT executed!
    Before(Action<A>),
    /// Exit with an error.
    ExitWithError(A::ExitError),
    /// Exit normally.
    NormalExit(A::ExitNormal),
}

/// The flow for handling actor initialisation. Any error can be propagated here to return an
/// `InitFlow::Error`. If an `InitFlow::Error` is returned however, the `handle_exit()` callback
/// will NOT be called. Instead the caller will get a `ProcessExit::InitFailed` response directly.
pub enum InitFlow<A: Actor> {
    /// Ok.
    Init(A),
    /// Initialisation ok, and right before handling the next message, execute this 'Action`.
    InitAndBefore(A, Action<A>),
    /// Exit with an error. This error is directly returned to the `Process`, without calling 
    /// `handle_exit()`.
    Exit,
}

/// The flow for handling an actor exiting. This can either continue the exit, or resume the actor 
/// with a new state. Errors can not be propagated, but must be handled here and turned into either 
/// an `A::Exit`, or a resume.
pub enum ExitFlow<A: Actor> {
    /// Continue this exit, with exit value `Actor::Exit`.
    ContinueExit(A::ExitWith),
    /// Resume execution of this actor.
    Resume(A),
    /// Resume execution of this actor. 
    /// Right before handling the next message, execute this 'Action`.
    ResumeAndBefore(A, Action<A>),
}

/// This is an internally used flow. ReqFlows and Flows are converted into this one,
/// so that the actor can easily handle them
pub(crate) enum InternalFlow<A: Actor + ?Sized> {
    Ok,
    After(Action<A>),
    Before(Action<A>),
    ErrorExit(A::ExitError),
    NormalExit(A::ExitNormal),
}

//--------------------------------------------------------------------------------------------------
//  Implement Flows
//--------------------------------------------------------------------------------------------------

impl<A: Actor, R> ReqFlow<A, R> {
    pub(crate) fn take_reply(self) -> (InternalFlow<A>, Option<R>) {
        match self {
            ReqFlow::Reply(reply) => (InternalFlow::Ok, Some(reply)),
            ReqFlow::ReplyAndAfter(reply, handler) => (InternalFlow::After(handler), Some(reply)),
            ReqFlow::ReplyAndBefore(reply, handler) => (InternalFlow::Before(handler), Some(reply)),
            ReqFlow::ReplyAndExitWithError(reply, error) => {
                (InternalFlow::ErrorExit(error), Some(reply))
            }
            ReqFlow::ExitWithError(error) => (InternalFlow::ErrorExit(error), None),
            ReqFlow::ReplyAndNormalExit(reply, normal) => {
                (InternalFlow::NormalExit(normal), Some(reply))
            }
            ReqFlow::NormalExit(normal) => (InternalFlow::NormalExit(normal), None),
        }
    }
}

impl<A: Actor> MsgFlow<A> {
    pub(crate) fn into_internal(self) -> InternalFlow<A> {
        match self {
            MsgFlow::Ok => InternalFlow::Ok,
            MsgFlow::Before(handler) => InternalFlow::Before(handler),
            MsgFlow::ExitWithError(error) => InternalFlow::ErrorExit(error),
            MsgFlow::NormalExit(normal) => InternalFlow::NormalExit(normal),
        }
    }
}

//--------------------------------------------------------------------------------------------------
//  IntoReply
//--------------------------------------------------------------------------------------------------

/// Applying the `?`-operating will propagate this error to the caller.
#[must_use = "This error must be used!"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntoReply<T, E> {
    Continue(T),
    Stop(E),
}

/// A trait for converting results to `IntoReply`
pub trait IntoReplyAble<T, E> {
    /// Applying the `?`-operating will propagate this error to the caller.
    fn into_reply(self) -> IntoReply<T, E>;
}

impl<T, E> IntoReplyAble<T, E> for Result<T, E> {
    /// Applying the `?`-operating will propagate this error to the caller.
    fn into_reply(self) -> IntoReply<T, E> {
        match self {
            Ok(t) => IntoReply::Continue(t),
            Err(e) => IntoReply::Stop(e),
        }
    }
}

impl<T, E> Try for IntoReply<T, E> {
    type Output = T;

    type Residual = IntoReply<Infallible, E>;

    fn from_output(output: Self::Output) -> Self {
        Self::Continue(output)
    }

    fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
        match self {
            IntoReply::Continue(t) => ControlFlow::Continue(t),
            IntoReply::Stop(e) => ControlFlow::Break(IntoReply::Stop(e)),
        }
    }
}

//--------------------------------------------------------------------------------------------------
//  FromResidual implementations
//--------------------------------------------------------------------------------------------------

impl<T, E> FromResidual<IntoReply<Infallible, E>> for IntoReply<T, E> {
    fn from_residual(residual: IntoReply<Infallible, E>) -> Self {
        match residual {
            IntoReply::Continue(_inf) => unreachable!(),
            IntoReply::Stop(e) => Self::Stop(e),
        }
    }
}

impl<A: Actor, R, E> FromResidual<Result<Infallible, E>> for ReqFlow<A, R>
where
    E: Into<A::ExitError>,
{
    fn from_residual(residual: Result<Infallible, E>) -> Self {
        match residual {
            Ok(_inf) => unreachable!(),
            Err(e) => ReqFlow::ExitWithError(e.into()),
        }
    }
}

impl<A: Actor, E, F, T> FromResidual<IntoReply<Infallible, E>> for ReqFlow<A, Result<T, F>>
where
    F: From<E>,
{
    fn from_residual(residual: IntoReply<Infallible, E>) -> Self {
        match residual {
            IntoReply::Continue(_inf) => unreachable!(),
            IntoReply::Stop(e) => ReqFlow::Reply(Err(e.into())),
        }
    }
}

impl<A: Actor, E> FromResidual<Result<Infallible, E>> for MsgFlow<A>
where
    E: Into<A::ExitError>,
{
    fn from_residual(residual: Result<Infallible, E>) -> Self {
        match residual {
            Ok(_inf) => unreachable!(),
            Err(e) => MsgFlow::ExitWithError(e.into()),
        }
    }
}

impl<A: Actor, E> FromResidual<Result<Infallible, E>> for InitFlow<A> {
    fn from_residual(residual: Result<Infallible, E>) -> Self {
        match residual {
            Ok(_inf) => unreachable!(),
            Err(e) => InitFlow::Exit,
        }
    }
}

impl<A: Actor> FromResidual<Option<Infallible>> for InitFlow<A> {
    fn from_residual(residual: Option<Infallible>) -> Self {
        match residual {
            Some(_inf) => unreachable!(),
            None => InitFlow::Exit,
        }
    }
}
