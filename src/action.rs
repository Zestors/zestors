use crate::{
    actor::Actor,
    flows::{EventFlow, MsgFlow, ReqFlow},
    function::{AnyFn, HandlerParams, MsgFn, ReqFn},
    messaging::{Reply, Request},
};
use futures::{Future, FutureExt};
use std::{
    any::Any, fmt::Debug, intrinsics::transmute, marker::PhantomData, pin::Pin, process::Output,
};

//--------------------------------------------------------------------------------------------------
//  helper types
//--------------------------------------------------------------------------------------------------

pub struct MsgFnId;
pub(crate) type MsgFnType<'a, A, P> = fn(&'a mut A, P) -> MsgFlow<A>;
pub struct AsyncMsgFnId;
pub(crate) type AsyncMsgFnType<'a, A, P, F> = fn(&'a mut A, P) -> F;

pub struct ReqFnId;
pub(crate) type ReqFnType<'a, A, P, R> = fn(&'a mut A, P) -> ReqFlow<A, R>;
pub struct AsyncReqFnId;
pub(crate) type AsyncReqFnType<'a, A, P, F> = fn(&'a mut A, P) -> F;

pub struct ReqFn2Id;
pub(crate) type ReqFn2Type<'a, A, P, R> = fn(&'a mut A, P, Request<R>) -> MsgFlow<A>;
pub struct AsyncReqFn2Id;
pub(crate) type AsyncReqFn2Type<'a, A, P, R, F> = fn(&'a mut A, P, Request<R>) -> F;

pub(crate) type HandlerFnType<A> = fn(&mut A, HandlerParams, usize) -> EventFlow<A>;
pub(crate) type AsyncHandlerFnType<A> =
    fn(
        &mut A,
        HandlerParams,
        usize,
    ) -> Pin<Box<dyn Future<Output = EventFlow<A>> + Send + 'static>>;

//------------------------------------------------------------------------------------------------
//  Action2
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Action<A: ?Sized> {
    function: AnyFn<A>,
    params: HandlerParams,
}

impl<A: Actor> Action<A> {
    pub fn new<P: Send + 'static>(function: MsgFn<A, P>, params: P) -> Self {
        Self {
            function: function.into(),
            params: HandlerParams::new_msg(params),
        }
    }

    pub(crate) fn new_req<P: Send + 'static, R: Send + 'static>(
        function: ReqFn<A, P, R>,
        params: P,
        request: Request<R>,
    ) -> Self {
        Self {
            function: function.into(),
            params: HandlerParams::new_req(params, request),
        }
    }

    pub async fn handle(self, actor: &mut A) -> EventFlow<A> {
        unsafe { self.function.call(actor, self.params).await }
    }

    pub(crate) fn downcast_req<P: Send + 'static, R: Send + 'static>(self) -> Result<P, Self> {
        match self.params.downcast_req::<P, R>() {
            Ok((params, _request)) => Ok(params),
            Err(boxed) => Err(Self {
                function: self.function,
                params: unsafe { HandlerParams::new_raw(boxed) }
            }),
        }
    }

    pub fn downcast<P: Send + 'static>(self) -> Result<P, Self> {
        match self.params.downcast_msg() {
            Ok(params) => Ok(params),
            Err(boxed) => Err(Self {
                function: self.function,
                params: unsafe { HandlerParams::new_raw(boxed) }
            }),
        }
    }
}