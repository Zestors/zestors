use crate::{
    actor::Actor,
    flows::{EventFlow, MsgFlow, ReqFlow},
    function::{AnyFn, HandlerParams, MsgFn, ReqFn},
    messaging::Request,
};
use futures::Future;
use std::{fmt::Debug, marker::PhantomData, pin::Pin};

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
//  Action
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Action<A: ?Sized> {
    function: AnyFn,
    params: HandlerParams,
    a: PhantomData<A>,
}

impl<A: Actor> Action<A> {
    pub fn new<P: Send + 'static>(function: MsgFn<A, P>, params: P) -> Self {
        Self {
            function: function.into(),
            params: HandlerParams::new_msg(params),
            a: PhantomData,
        }
    }

    pub fn new_req<P: Send + 'static, R: Send + 'static>(
        function: ReqFn<A, P, R>,
        params: P,
        request: Request<R>,
    ) -> Self {
        Self {
            function: function.into(),
            params: HandlerParams::new_req(params, request),
            a: PhantomData,
        }
    }

    pub fn downcast_req<P: Send + 'static, R: Send + 'static>(
        self,
    ) -> Result<(P, Request<R>), Self> {
        match self.params.downcast_req::<P, R>() {
            Ok((params, request)) => Ok((params, request)),
            Err(boxed) => Err(Self {
                function: self.function,
                params: unsafe { HandlerParams::new_raw(boxed) },
                a: PhantomData,
            }),
        }
    }

    pub fn downcast<P: Send + 'static>(self) -> Result<P, Self> {
        match self.params.downcast_msg() {
            Ok(params) => Ok(params),
            Err(boxed) => Err(Self {
                function: self.function,
                params: unsafe { HandlerParams::new_raw(boxed) },
                a: PhantomData,
            }),
        }
    }

    pub async fn handle(self, actor: &mut A) -> EventFlow<A> {
        unsafe { self.function.call(actor, self.params).await }
    }

    pub(crate) unsafe fn new_raw_msg<P: Send + 'static>(function: AnyFn, params: P) -> Self {
        Self {
            function,
            params: HandlerParams::new_msg(params),
            a: PhantomData,
        }
    }

    pub(crate) unsafe fn new_raw_req<P: Send + 'static, R: Send + 'static>(
        function: AnyFn,
        params: P,
        request: Request<R>,
    ) -> Self {
        Self {
            function,
            params: HandlerParams::new_req(params, request),
            a: PhantomData,
        }
    }
}
