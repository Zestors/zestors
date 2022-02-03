use std::{
    convert::Infallible,
    ops::{ControlFlow, FromResidual, Try},
    time::Duration,
};

use crate::actor::Actor;


/// This indicates a request that was made by a caller.
/// It should normally be replied to, but the reply can also be ignored.
///
/// Errors are normally transferred directly to the reply, for example if you have a
/// `ReqFlow<Self, anyhow::Error>`, using the `?` withing this function will return back an
/// error to the caller.
///
/// Errors can be converted to stopping the actor by calling `.stop()?` on the `Result<T, E>`.
/// The caller will receive a message that it's `Req` has been dropped, and the `stopping`
/// function on the actor trait will be called.
pub enum ReqFlow<A: Actor, R> {
    /// Send back a reply to the caller
    Reply(R),
    /// Send back a reply to the caller, and change configurations of the actor
    ReplyAnd(R, FlowConfig),

    /// Ignore the callers request. The caller will receive a message indicating it's request
    /// has been dropped.
    Ignore,
    /// Ignore the callers request. The caller will receive a message indicating it's request
    /// has been dropped. Also change the configuration of the actor
    IgnoreAnd(FlowConfig),

    /// Stop the actor with an error message. The caller will receive a message indicating it's request
    /// has been dropped. `stopping` will subsequently be called with this
    /// value
    Stop(A::Error),
    /// Stop the actor with an error message. The caller will still receive the request before `stopping`
    /// is called. `stopping` will subsequently be called with the error value
    ReplyAndStop(R, A::Error),

    /// Stop the actor without an error. The caller will receive a message indicating it's request
    /// has been dropped. `stopping` will subsequently be called with a `Normal`
    /// value
    StopNormal,
    /// Stop the actor without an error. The caller will receive the reply.
    /// stopping` will subsequently be called with a `Normal` value
    ReplyAndStopNormal(R),
}

impl<A: Actor, R> ReqFlow<A, R> {
    pub fn take_reply(self) -> (MsgFlow<A>, Option<R>) {
        match self {
            ReqFlow::Reply(r) => (MsgFlow::Ok, Some(r)),
            ReqFlow::ReplyAnd(r, c) => (MsgFlow::OkAnd(c), Some(r)),
            ReqFlow::Ignore => (MsgFlow::Ok, None),
            ReqFlow::IgnoreAnd(c) => (MsgFlow::OkAnd(c), None),
            ReqFlow::Stop(e) => (MsgFlow::Stop(e), None),
            ReqFlow::ReplyAndStop(r, e) => (MsgFlow::Stop(e), Some(r)),
            ReqFlow::StopNormal => (MsgFlow::StopNormal, None),
            ReqFlow::ReplyAndStopNormal(r) => (MsgFlow::StopNormal, Some(r)),
        }
    }
}

/// This indicates a message that was sent by a caller. It does not need a reply.
///
/// Errors can be converted to stopping the actor by calling `.stop()?` on the `Result<T, E>`.
/// The `stopping` function on the actor trait will be subsequently called.
pub enum MsgFlow<A: Actor + ?Sized> {
    /// End of handling this msg
    Ok,
    /// End of handling this msg, and change configuration of the actor
    OkAnd(FlowConfig),

    /// Stop the actor with an error message, calling `stopping` afterwards with this value.
    Stop(A::Error),
    /// Stop the actor without error msg, calling `stopping` afterwards with `Normal` value.
    StopNormal,
}

unsafe impl<A: Actor> Send for MsgFlow<A> {}
// unsafe impl<A: Actor> Send for Pin<Box<dyn futures::Future<Output = MsgFlow<A>>>> {}

pub struct FlowConfig {}

//-------------------------------------
// IntoStop Trait
//-------------------------------------

pub enum IntoStop<T, E> {
    Continue(T),
    Stop(E),
}

pub trait Stop<T, E> {
    fn stop(self) -> IntoStop<T, E>;
}

impl<T, E> Stop<T, E> for Result<T, E> {
    fn stop(self) -> IntoStop<T, E> {
        match self {
            Ok(t) => IntoStop::Continue(t),
            Err(e) => IntoStop::Stop(e),
        }
    }
}

//-------------------------------------
// Try implementations
//-------------------------------------

impl<T, E> Try for IntoStop<T, E> {
    type Output = T;

    type Residual = IntoStop<Infallible, E>;

    fn from_output(output: Self::Output) -> Self {
        Self::Continue(output)
    }

    fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
        match self {
            IntoStop::Continue(t) => ControlFlow::Continue(t),
            IntoStop::Stop(e) => ControlFlow::Break(IntoStop::Stop(e)),
        }
    }
}

impl<T, E> FromResidual<IntoStop<Infallible, E>> for IntoStop<T, E> {
    fn from_residual(residual: IntoStop<Infallible, E>) -> Self {
        match residual {
            IntoStop::Continue(_inf) => unreachable!(),
            IntoStop::Stop(e) => Self::Stop(e),
        }
    }
}

//-------------------------------------
// FromResidual<IntoStop>
//-------------------------------------

impl<A: Actor, R, E> FromResidual<IntoStop<Infallible, E>> for ReqFlow<A, R>
where
    E: Into<A::Error>,
{
    fn from_residual(residual: IntoStop<Infallible, E>) -> Self {
        match residual {
            IntoStop::Continue(_inf) => unreachable!(),
            IntoStop::Stop(e) => ReqFlow::Stop(e.into()),
        }
    }
}

impl<A: Actor, E> FromResidual<IntoStop<Infallible, E>> for MsgFlow<A>
where
    E: Into<A::Error>,
{
    fn from_residual(residual: IntoStop<Infallible, E>) -> Self {
        match residual {
            IntoStop::Continue(_inf) => unreachable!(),
            IntoStop::Stop(e) => MsgFlow::Stop(e.into()),
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
