use std::{
    any::Any,
    convert::Infallible,
    intrinsics::transmute,
    ops::{ControlFlow, FromResidual, Try},
    pin::Pin,
    time::Duration,
};

use crate::{actor::Actor, action::{After, Before}};
use futures::Future;



/// Flow for handling a request
pub enum ReqFlow<A: Actor + ?Sized, R> {
    Reply(R),
    Ignore,
    ReplyAndAfter(R, After<A>),
    IgnoreAndAfter(After<A>),
    ReplyAndBefore(R, Before<A>),
    IgnoreAndBefore(Before<A>),
    ReplyAndErrorExit(R, A::ErrorExit),
    IgnoreAndErrorExit(A::ErrorExit),
    ReplyAndNormalExit(R, A::NormalExit),
    IgnoreAndNormalExit(A::NormalExit),
}

/// Flow for handling a msg, or when starting this actor
pub enum MsgFlow<A: Actor + ?Sized> {
    Ok,
    Before(Before<A>),
    ErrorExit(A::ErrorExit),
    NormalExit(A::NormalExit),
}

/// Flow for handling a scheduled future, or before handling the next action
pub enum Flow<A: Actor + ?Sized> {
    Ok,
    ErrorExit(A::ErrorExit),
    NormalExit(A::NormalExit),
}

/// The internal flow, which 
pub(crate) enum InternalFlow<A: Actor + ?Sized> {
    Ok,

    After(After<A>),

    Before(Before<A>),

    ErrorExit(A::ErrorExit),

    NormalExit(A::NormalExit),
}

impl<A: Actor, R> ReqFlow<A, R> {
    pub(crate) fn take_reply(self) -> (InternalFlow<A>, Option<R>) {
        match self {
            ReqFlow::Reply(reply) => (InternalFlow::Ok, Some(reply)),
            ReqFlow::Ignore => (InternalFlow::Ok, None),
            ReqFlow::ReplyAndAfter(reply, handler) => (InternalFlow::After(handler), Some(reply)),
            ReqFlow::IgnoreAndAfter(handler) => (InternalFlow::After(handler), None),
            ReqFlow::ReplyAndBefore(reply, handler) => (InternalFlow::Before(handler), Some(reply)),
            ReqFlow::IgnoreAndBefore(handler) => (InternalFlow::Before(handler), None),
            ReqFlow::ReplyAndErrorExit(reply, error) => (InternalFlow::ErrorExit(error), Some(reply)),
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
// IntoStop Trait
//-------------------------------------

pub enum IntoExit<T, E> {
    Continue(T),
    Stop(E),
}

pub trait Exit<T, E> {
    fn into_exit(self) -> IntoExit<T, E>;
}

impl<T, E> Exit<T, E> for Result<T, E> {
    fn into_exit(self) -> IntoExit<T, E> {
        match self {
            Ok(t) => IntoExit::Continue(t),
            Err(e) => IntoExit::Stop(e),
        }
    }
}

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
    E: Into<A::ErrorExit>,
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
    E: Into<A::ErrorExit>,
{
    fn from_residual(residual: IntoExit<Infallible, E>) -> Self {
        match residual {
            IntoExit::Continue(_inf) => unreachable!(),
            IntoExit::Stop(e) => MsgFlow::ErrorExit(e.into()),
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
