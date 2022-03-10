use crate::{
    action::{
        AsyncHandlerFnType, AsyncMsgFnId, AsyncMsgFnType, AsyncReqFn2Id, AsyncReqFn2Type,
        AsyncReqFnId, AsyncReqFnType, HandlerFnType, MsgFnId, MsgFnType, ReqFn2Id, ReqFn2Type,
        ReqFnId, ReqFnType,
    },
    actor::Actor,
    distributed::node::NodeActor,
    flows::{EventFlow, MsgFlow, ReqFlow},
    messaging::{Reply, Request},
};
use futures::{Future, FutureExt};
use serde::{Deserialize, Serialize};
use std::{any::Any, marker::PhantomData, mem::transmute, pin::Pin};

//------------------------------------------------------------------------------------------------
//  HandlerPtr
//------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum HandlerPtr {
    Sync(usize),
    Async(usize),
}

impl HandlerPtr {
    pub(crate) fn new_sync<A: Actor>(handler: HandlerFnType<A>) -> Self {
        Self::Sync(handler as usize)
    }

    pub(crate) fn new_async<A: Actor>(handler: AsyncHandlerFnType<A>) -> Self {
        Self::Async(handler as usize)
    }
}

//------------------------------------------------------------------------------------------------
//  Fn!
//------------------------------------------------------------------------------------------------

#[macro_export]
macro_rules! Fn {
    ($function:expr) => {
        unsafe {
            <() as $crate::function::IntoFn<_, _, _, _, _, _, _>>::into_fn(
                $function as usize,
                $function,
            )
        }
    };
}

//------------------------------------------------------------------------------------------------
//  AnyFn
//------------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AnyFn<A: ?Sized> {
    function: usize,
    handler: HandlerPtr,
    a: PhantomData<A>,
}

impl<A: Actor> AnyFn<A> {
    pub(crate) async unsafe fn call(&self, actor: &mut A, params: HandlerParams) -> EventFlow<A> {
        match self.handler {
            HandlerPtr::Async(handler) => {
                let handler: AsyncHandlerFnType<A> = unsafe { transmute(handler) };
                handler(actor, params, self.function).await
            }
            HandlerPtr::Sync(handler) => {
                let handler: HandlerFnType<A> = unsafe { transmute(handler) };
                handler(actor, params, self.function)
            }
        }
    }
}

impl<A, P, R> From<ReqFn<A, P, R>> for AnyFn<A> {
    fn from(ptr: ReqFn<A, P, R>) -> Self {
        Self {
            function: ptr.function,
            handler: ptr.handler,
            a: PhantomData,
        }
    }
}

impl<A, P> From<MsgFn<A, P>> for AnyFn<A> {
    fn from(ptr: MsgFn<A, P>) -> Self {
        Self {
            function: ptr.function,
            handler: ptr.handler,
            a: PhantomData,
        }
    }
}

//------------------------------------------------------------------------------------------------
//  HandlerParams
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) struct HandlerParams(Box<dyn Any + Send>);

impl HandlerParams {
    pub fn new_req<P: Send + 'static, R: Send + 'static>(params: P, request: Request<R>) -> Self {
        Self(Box::new((params, request)))
    }

    pub fn new_msg<P: Send + 'static>(params: P) -> Self {
        Self(Box::new(params))
    }

    pub(crate) unsafe fn new_raw(inner: Box<dyn Any + Send>) -> Self {
        Self(inner)
    }

    pub fn downcast_req<P: Send + 'static, R: Send + 'static>(
        self,
    ) -> Result<(P, Request<R>), Box<dyn Any + Send>> {
        self.0.downcast().map(|p| *p)
    }

    pub fn downcast_msg<P: Send + 'static>(self) -> Result<P, Box<dyn Any + Send>> {
        self.0.downcast().map(|p| *p)
    }
}

//------------------------------------------------------------------------------------------------
//  ReqFn
//------------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ReqFn<A: ?Sized, P: ?Sized, R: ?Sized> {
    a: PhantomData<A>,
    p: PhantomData<P>,
    r: PhantomData<R>,
    function: usize,
    handler: HandlerPtr,
}

impl<A: Actor, P: Send + 'static, R: Send + 'static> ReqFn<A, P, R> {
    pub(crate) async unsafe fn call(&self, actor: &mut A, params: P, request: Request<R>) -> EventFlow<A> {
        unsafe {
            self.call_unchecked(actor, HandlerParams::new_req(params, request))
                .await
        }
    }

    pub(crate) async unsafe fn call_unchecked(
        &self,
        actor: &mut A,
        params: HandlerParams,
    ) -> EventFlow<A> {
        match self.handler {
            HandlerPtr::Async(handler) => {
                let handler: AsyncHandlerFnType<A> = unsafe { transmute(handler) };
                handler(actor, params, self.function).await
            }
            HandlerPtr::Sync(handler) => {
                let handler: HandlerFnType<A> = unsafe { transmute(handler) };
                handler(actor, params, self.function)
            }
        }
    }

    pub fn new_sync(function: ReqFnType<A, P, R>) -> Self {
        Self {
            r: PhantomData,
            a: PhantomData,
            p: PhantomData,
            function: function as usize,
            handler: HandlerPtr::new_sync(Self::__sync_handler__),
        }
    }

    fn __sync_handler__(actor: &mut A, params: HandlerParams, actual_fn: usize) -> EventFlow<A> {
        let function: ReqFnType<A, P, R> = unsafe { transmute(actual_fn) };

        let (params, request) = params.downcast_req().unwrap();

        let (flow, reply) = function(actor, params).take_reply();

        if let Some(reply) = reply {
            request.reply(reply)
        }

        flow
    }

    pub fn new_async<F>(function: AsyncReqFnType<A, P, F>) -> Self
    where
        F: Future<Output = ReqFlow<A, R>> + Send + 'static,
    {
        Self {
            r: PhantomData,
            a: PhantomData,
            p: PhantomData,
            function: function as usize,
            handler: HandlerPtr::new_async(Self::__async_handler__::<F>),
        }
    }

    fn __async_handler__<F: Future<Output = ReqFlow<A, R>> + Send + 'static>(
        actor: &mut A,
        params: HandlerParams,
        actual_fn: usize,
    ) -> Pin<Box<dyn Future<Output = EventFlow<A>> + Send + 'static>> {
        let function: AsyncReqFnType<A, P, F> = unsafe { transmute(actual_fn) };

        let (params, request) = params.downcast_req().unwrap();

        Box::pin(function(actor, params).map(|f| {
            let (flow, reply) = f.take_reply();

            if let Some(reply) = reply {
                request.reply(reply);
            }

            flow
        }))
    }

    pub fn new_sync2(function: ReqFn2Type<A, P, R>) -> Self {
        Self {
            r: PhantomData,
            a: PhantomData,
            p: PhantomData,
            function: function as usize,
            handler: HandlerPtr::new_sync(Self::__sync2_handler__),
        }
    }

    fn __sync2_handler__(actor: &mut A, params: HandlerParams, actual_fn: usize) -> EventFlow<A> {
        let function: ReqFn2Type<A, P, R> = unsafe { transmute(actual_fn) };

        let (params, request) = params.downcast_req().unwrap();

        function(actor, params, request).into_event_flow()
    }

    pub fn new_async2<F>(function: AsyncReqFn2Type<A, P, R, F>) -> Self
    where
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        Self {
            r: PhantomData,
            a: PhantomData,
            p: PhantomData,
            function: function as usize,
            handler: HandlerPtr::new_async(Self::__async2_handler__::<F>),
        }
    }

    fn __async2_handler__<F>(
        actor: &mut A,
        params: HandlerParams,
        actual_fn: usize,
    ) -> Pin<Box<dyn Future<Output = EventFlow<A>> + Send + 'static>>
    where
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        let function: AsyncReqFn2Type<A, P, R, F> = unsafe { transmute(actual_fn) };

        let (params, request): (P, Request<R>) = params.downcast_req().unwrap();

        Box::pin(function(actor, params, request).map(|f| f.into_event_flow()))
    }
}

//------------------------------------------------------------------------------------------------
//  MsgFn
//------------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct MsgFn<A: ?Sized, P: ?Sized> {
    a: PhantomData<A>,
    p: PhantomData<P>,
    function: usize,
    handler: HandlerPtr,
}

impl<A: Actor, P: Send + 'static> MsgFn<A, P> {
    pub(crate) async unsafe fn call(&self, actor: &mut A, params: P) -> EventFlow<A> {
        unsafe { self.call_unchecked(actor, HandlerParams::new_msg(params)).await }
    }

    pub(crate) async unsafe fn call_unchecked(
        &self,
        actor: &mut A,
        params: HandlerParams,
    ) -> EventFlow<A> {
        match self.handler {
            HandlerPtr::Async(handler) => {
                let handler: AsyncHandlerFnType<A> = unsafe { transmute(handler) };
                handler(actor, params, self.function).await
            }
            HandlerPtr::Sync(handler) => {
                let handler: HandlerFnType<A> = unsafe { transmute(handler) };
                handler(actor, params, self.function)
            }
        }
    }

    pub fn new_sync(ptr: MsgFnType<A, P>) -> Self {
        Self {
            a: PhantomData,
            p: PhantomData,
            function: ptr as usize,
            handler: HandlerPtr::new_sync(Self::__sync_handler__),
        }
    }

    fn __sync_handler__(actor: &mut A, params: HandlerParams, actual_fn: usize) -> EventFlow<A> {
        let function: MsgFnType<A, P> = unsafe { transmute(actual_fn) };
        function(actor, params.downcast_msg().unwrap()).into_event_flow()
    }

    pub fn new_async<F>(ptr: AsyncMsgFnType<A, P, F>) -> Self
    where
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        Self {
            a: PhantomData,
            p: PhantomData,
            function: ptr as usize,
            handler: HandlerPtr::new_async(Self::__async_handler__::<F>),
        }
    }

    fn __async_handler__<F>(
        actor: &mut A,
        params: HandlerParams,
        actual_fn: usize,
    ) -> Pin<Box<dyn Future<Output = EventFlow<A>> + Send + 'static>>
    where
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        let function: AsyncMsgFnType<A, P, F> = unsafe { transmute(actual_fn) };

        Box::pin(function(actor, params.downcast_msg().unwrap()).map(|f| f.into_event_flow()))
    }
}

//------------------------------------------------------------------------------------------------
//  Implementations
//------------------------------------------------------------------------------------------------

impl<A> Clone for AnyFn<A> {
    fn clone(&self) -> Self {
        Self {
            a: self.a.clone(),
            function: self.function.clone(),
            handler: self.handler.clone(),
        }
    }
}
impl<A, P> Clone for MsgFn<A, P> {
    fn clone(&self) -> Self {
        Self {
            a: self.a.clone(),
            p: self.p.clone(),
            function: self.function.clone(),
            handler: self.handler.clone(),
        }
    }
}
impl<A, P, R> Clone for ReqFn<A, P, R> {
    fn clone(&self) -> Self {
        Self {
            a: self.a.clone(),
            p: self.p.clone(),
            function: self.function.clone(),
            handler: self.handler.clone(),
            r: self.r.clone()
        }
    }
}
impl<A> Copy for AnyFn<A> {}
impl<A, P> Copy for MsgFn<A, P> {}
impl<A, P, R> Copy for ReqFn<A, P, R> {}
unsafe impl<A> Send for AnyFn<A> {}
unsafe impl<A> Sync for AnyFn<A> {}
unsafe impl<A, P, R> Send for ReqFn<A, P, R> {}
unsafe impl<A, P, R> Sync for ReqFn<A, P, R> {}
unsafe impl<A, P> Send for MsgFn<A, P> {}
unsafe impl<A, P> Sync for MsgFn<A, P> {}

//------------------------------------------------------------------------------------------------
//  IntoFn
//------------------------------------------------------------------------------------------------

pub trait IntoFn<A, Fun, P, R, Fut, X, Id> {
    unsafe fn into_fn(function_ptr: usize, function: Fun) -> X;
}

impl<A, Fun, P> IntoFn<A, Fun, P, (), (), MsgFn<A, P>, MsgFnId> for ()
where
    Fun: Fn(&mut A, P) -> MsgFlow<A>,
    A: Actor,
    P: Send + 'static,
{
    unsafe fn into_fn(ptr: usize, _: Fun) -> MsgFn<A, P> {
        let function: MsgFnType<A, P> = transmute(ptr);
        MsgFn::new_sync(function)
    }
}

impl<'a, A, Fun, P, Fut> IntoFn<A, Fun, P, (), Fut, MsgFn<A, P>, AsyncMsgFnId> for ()
where
    Fun: Fn(&'a mut A, P) -> Fut,
    Fut: Future<Output = MsgFlow<A>> + 'static + MsgHelperTrait + Send,
    A: Actor,
    P: Send + 'static,
{
    unsafe fn into_fn(ptr: usize, _: Fun) -> MsgFn<A, P> {
        let function: AsyncMsgFnType<A, P, Fut> = transmute(ptr);
        MsgFn::new_async(function)
    }
}

impl<A, F, P, R> IntoFn<A, F, P, R, (), ReqFn<A, P, R>, ReqFnId> for ()
where
    F: Fn(&mut A, P) -> ReqFlow<A, R>,
    A: Actor,
    P: Send + 'static,
    R: Send + 'static,
{
    unsafe fn into_fn(ptr: usize, _: F) -> ReqFn<A, P, R> {
        let function: ReqFnType<A, P, R> = transmute(ptr);
        ReqFn::new_sync(function)
    }
}

impl<'a, A, Fun, P, Fut, R> IntoFn<A, Fun, P, R, Fut, ReqFn<A, P, R>, AsyncReqFnId> for ()
where
    Fun: Fn(&'a mut A, P) -> Fut,
    Fut: Future<Output = ReqFlow<A, R>> + 'static + ReqHelperTrait + Send,
    A: Actor,
    P: Send + 'static,
    R: Send + 'static,
{
    unsafe fn into_fn(ptr: usize, _: Fun) -> ReqFn<A, P, R> {
        let function: AsyncReqFnType<A, P, Fut> = transmute(ptr);
        ReqFn::new_async(function)
    }
}

impl<A, F, P, R> IntoFn<A, F, P, R, (), ReqFn<A, P, R>, ReqFn2Id> for ()
where
    F: Fn(&mut A, P, Request<R>) -> MsgFlow<A>,
    A: Actor,
    P: Send + 'static,
    R: Send + 'static,
{
    unsafe fn into_fn(ptr: usize, _: F) -> ReqFn<A, P, R> {
        let function: ReqFn2Type<A, P, R> = transmute(ptr);
        ReqFn::new_sync2(function)
    }
}

impl<'a, A, Fun, P, Fut, R> IntoFn<A, Fun, P, R, Fut, ReqFn<A, P, R>, AsyncReqFn2Id> for ()
where
    Fun: Fn(&'a mut A, P, Request<R>) -> Fut,
    Fut: Future<Output = MsgFlow<A>> + 'static + Send,
    A: Actor,
    P: Send + 'static,
    R: Send + 'static,
{
    unsafe fn into_fn(ptr: usize, _: Fun) -> ReqFn<A, P, R> {
        let function: AsyncReqFn2Type<A, P, R, Fut> = transmute(ptr);
        ReqFn::new_async2(function)
    }
}

//------------------------------------------------------------------------------------------------
//  MsgHelperTrait
//------------------------------------------------------------------------------------------------

pub trait MsgHelperTrait {}
pub trait ReqHelperTrait {}
impl<A: Actor, T: Future<Output = MsgFlow<A>>> MsgHelperTrait for T {}
impl<A: Actor, R, T: Future<Output = ReqFlow<A, R>>> ReqHelperTrait for T {}
