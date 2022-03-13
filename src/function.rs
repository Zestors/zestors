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
pub(crate) enum HandlerPtr {
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

#[derive(Debug)]
pub(crate) struct AnyFn {
    pub(crate) function: usize,
    pub(crate) handler: HandlerPtr,
}

impl AnyFn {
    pub(crate) async unsafe fn call<A: Actor>(
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
}

impl<'params, A, P, R, S> From<ReqFn<'params, A, P, R, S>> for AnyFn {
    fn from(ptr: ReqFn<A, P, R, S>) -> Self {
        Self {
            function: ptr.function,
            handler: ptr.handler,
        }
    }
}

impl<A, P> From<MsgFn<A, P>> for AnyFn {
    fn from(ptr: MsgFn<A, P>) -> Self {
        Self {
            function: ptr.function,
            handler: ptr.handler,
        }
    }
}

//------------------------------------------------------------------------------------------------
//  HandlerParams
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) enum HandlerParams {
    Box(Box<dyn Any + Send>),
    Ref(RefParams<'static>),
}

pub(crate) struct RefParams<'a>(Box<dyn Send + 'a>);

impl<'a> RefParams<'a> {
    pub fn new<P: Send + 'a, R: Send + 'a>(params: P, request: Request<R>) -> Self {
        Self(Box::new((params, request)))
    }

    pub unsafe fn transmute<P: Send + 'a, R: Send + 'a>(self) -> (P, Request<R>) {
        let (address, _metadata) = Box::into_raw(self.0).to_raw_parts();
        let boxed: Box<(P, Request<R>)> = transmute(address);
        *boxed
    }
}

unsafe impl Send for HandlerParams {}

impl HandlerParams {
    // pub fn new_req<P: Send + 'static, R: Send + 'static>(params: P, request: Request<R>) -> Self {
    //     Self::Box(Box::new((params, request)))
    // }

    pub fn new_msg<P: Send + 'static>(params: P) -> Self {
        Self::Box(Box::new(params))
    }

    pub unsafe fn new_req_ref<'a, P: Send + 'a, R: Send + 'a>(
        params: P,
        request: Request<R>,
    ) -> Self {
        let params = RefParams::new(params, request);
        let params: RefParams<'static> = std::mem::transmute(params);
        Self::Ref(params)
    }

    pub(crate) unsafe fn new_raw(inner: Box<dyn Any + Send>) -> Self {
        Self::Box(inner)
    }

    pub unsafe fn transmute_req_ref<'a, P: Send + 'a, R: Send + 'a>(self) -> (P, Request<R>) {
        match self {
            HandlerParams::Box(boxed) => panic!(),
            HandlerParams::Ref(boxed) => boxed.transmute(),
        }
    }

    // pub fn downcast_req<P: Send + 'static, R: Send + 'static>(
    //     self,
    // ) -> Result<(P, Request<R>), Box<dyn Any + Send>> {
    //     match self {
    //         HandlerParams::Box(boxed) => boxed.downcast().map(|p| *p),
    //         HandlerParams::Ref(boxed) => panic!(),
    //     }
    // }

    pub fn downcast_msg<P: Send + 'static>(self) -> Result<P, Box<dyn Any + Send>> {
        match self {
            HandlerParams::Box(boxed) => boxed.downcast().map(|p| *p),
            HandlerParams::Ref(_) => panic!(),
        }
    }
}

impl<'a> std::fmt::Debug for RefParams<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RefParams").field(&"").finish()
    }
}

//------------------------------------------------------------------------------------------------
//  ReqFn
//------------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ReqFn<'params, Actor: ?Sized, Params: ?Sized, Reply: ?Sized, RefSafe: ?Sized> {
    a: PhantomData<Actor>,
    p: PhantomData<&'params Params>,
    r: PhantomData<&'params Reply>,
    st: PhantomData<RefSafe>,
    function: usize,
    handler: HandlerPtr,
}

pub struct RefSafe;
pub struct RefUnsafe;

impl<A: Actor, P: Send + 'static, R: Send + 'static> ReqFn<'static, A, P, R, RefUnsafe> {
    pub fn new_sync2(function: ReqFn2Type<A, P, R>) -> Self {
        Self {
            r: PhantomData,
            a: PhantomData,
            p: PhantomData,
            st: PhantomData,
            function: function as usize,
            handler: HandlerPtr::new_sync(Self::__sync2_handler__),
        }
    }

    fn __sync2_handler__(actor: &mut A, params: HandlerParams, actual_fn: usize) -> EventFlow<A> {
        let function: ReqFn2Type<A, P, R> = unsafe { transmute(actual_fn) };

        let (params, request) = unsafe { params.transmute_req_ref() };

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
            st: PhantomData,
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

        let (params, request): (P, Request<R>) = unsafe { params.transmute_req_ref() };

        Box::pin(function(actor, params, request).map(|f| f.into_event_flow()))
    }
}

impl<'params, A, P, R> From<ReqFn<'params, A, P, R, RefSafe>>
    for ReqFn<'params, A, P, R, RefUnsafe>
{
    fn from(req: ReqFn<A, P, R, RefSafe>) -> Self {
        unsafe { std::mem::transmute(req) }
    }
}

impl<'params, A: Actor, P: Send + 'params, R: Send + 'params> ReqFn<'params, A, P, R, RefSafe> {
    pub(crate) async unsafe fn call(
        &self,
        actor: &mut A,
        params: P,
        request: Request<R>,
    ) -> EventFlow<A> {
        unsafe {
            self.call_unchecked(actor, HandlerParams::new_req_ref(params, request))
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

    pub fn new_sync(function: ReqFnType<'params, A, P, R>) -> Self {
        Self {
            r: PhantomData,
            a: PhantomData,
            p: PhantomData,
            st: PhantomData,
            function: function as usize,
            handler: HandlerPtr::new_sync(Self::__sync_handler__),
        }
    }

    fn __sync_handler__(actor: &mut A, params: HandlerParams, actual_fn: usize) -> EventFlow<A> {
        let function: ReqFnType<A, P, R> = unsafe { transmute(actual_fn) };

        let (params, request) = unsafe { params.transmute_req_ref() };

        let (flow, reply) = function(actor, params).take_reply();

        if let Some(reply) = reply {
            let _ = request.reply(reply);
        }

        flow
    }

    pub fn new_async<F>(function: AsyncReqFnType<A, P, F>) -> Self
    where
        F: Future<Output = ReqFlow<A, R>> + Send + 'params,
        A: 'params,
        P: 'params,
    {
        Self {
            r: PhantomData,
            a: PhantomData,
            p: PhantomData,
            st: PhantomData,
            function: function as usize,
            handler: HandlerPtr::new_async(Self::__async_handler__::<F>),
        }
    }

    fn __async_handler__<F: Future<Output = ReqFlow<A, R>> + Send + 'params>(
        actor: &mut A,
        params: HandlerParams,
        actual_fn: usize,
    ) -> Pin<Box<dyn Future<Output = EventFlow<A>> + Send + 'params>> {
        let function: AsyncReqFnType<A, P, F> = unsafe { transmute(actual_fn) };

        let (params, request) = unsafe { params.transmute_req_ref() };

        Box::pin(function(actor, params).map(|f| {
            let (flow, reply) = f.take_reply();

            if let Some(reply) = reply {
                let _ = request.reply(reply);
            }

            flow
        }))
    }
}

//------------------------------------------------------------------------------------------------
//  MsgFn
//------------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct MsgFn<Actor: ?Sized, Params: ?Sized> {
    a: PhantomData<Actor>,
    p: PhantomData<Params>,
    function: usize,
    handler: HandlerPtr,
}

impl<A: Actor, P: Send + 'static> MsgFn<A, P> {
    pub(crate) async unsafe fn call(&self, actor: &mut A, params: P) -> EventFlow<A> {
        unsafe {
            self.call_unchecked(actor, HandlerParams::new_msg(params))
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

impl Clone for AnyFn {
    fn clone(&self) -> Self {
        Self {
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
impl<'params, A, P, R> Clone for ReqFn<'params, A, P, R, RefSafe> {
    fn clone(&self) -> Self {
        Self {
            a: self.a.clone(),
            p: self.p.clone(),
            st: self.st.clone(),
            function: self.function.clone(),
            handler: self.handler.clone(),
            r: self.r.clone(),
        }
    }
}
impl Copy for AnyFn {}
impl<A, P> Copy for MsgFn<A, P> {}
impl<'params, A, P, R> Copy for ReqFn<'params, A, P, R, RefSafe> {}
unsafe impl Send for AnyFn {}
unsafe impl Sync for AnyFn {}
unsafe impl<'params, A, P, R> Send for ReqFn<'params, A, P, R, RefSafe> {}
unsafe impl<'params, A, P, R> Sync for ReqFn<'params, A, P, R, RefSafe> {}
unsafe impl<A, P> Send for MsgFn<A, P> {}
unsafe impl<A, P> Sync for MsgFn<A, P> {}

//------------------------------------------------------------------------------------------------
//  IntoFn
//------------------------------------------------------------------------------------------------

pub trait IntoFn<A, Fun, P, R, Fut, X, Id> {
    unsafe fn into_fn(function_ptr: usize, function: Fun) -> X;
}

impl<'a, 'f, A, Fun, P> IntoFn<A, Fun, P, (), (), MsgFn<A, P>, MsgFnId> for ()
where
    Fun: Fn(&'a mut A, P) -> MsgFlow<A>,
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

impl<'params, A, F, P, R> IntoFn<A, F, P, R, (), ReqFn<'params, A, P, R, RefSafe>, ReqFnId> for ()
where
    F: Fn(&'params mut A, P) -> ReqFlow<A, R>,
    A: Actor,
    P: Send + 'params,
    R: Send + 'params,
{
    unsafe fn into_fn(ptr: usize, _: F) -> ReqFn<'params, A, P, R, RefSafe> {
        let function: ReqFnType<A, P, R> = transmute(ptr);
        ReqFn::new_sync(function)
    }
}

impl<'params, A, Fun, P, Fut, R> IntoFn<A, Fun, P, R, Fut, ReqFn<'params, A, P, R, RefSafe>, AsyncReqFnId> for ()
where
    Fun: Fn(&'params mut A, P) -> Fut,
    Fut: Future<Output = ReqFlow<A, R>> + 'params + ReqHelperTrait + Send,
    A: Actor,
    P: Send + 'params,
    R: Send + 'params,
{
    unsafe fn into_fn(ptr: usize, _: Fun) -> ReqFn<'params, A, P, R, RefSafe> {
        let function: AsyncReqFnType<A, P, Fut> = transmute(ptr);
        ReqFn::new_async(function)
    }
}

impl<'a, A, F, P, R> IntoFn<A, F, P, R, (), ReqFn<'static, A, P, R, RefUnsafe>, ReqFn2Id> for ()
where
    F: Fn(&'a mut A, P, Request<R>) -> MsgFlow<A>,
    A: Actor,
    P: Send + 'static,
    R: Send + 'static,
{
    unsafe fn into_fn(ptr: usize, _: F) -> ReqFn<'static, A, P, R, RefUnsafe> {
        let function: ReqFn2Type<A, P, R> = transmute(ptr);
        ReqFn::new_sync2(function)
    }
}

impl<'a, A, Fun, P, Fut, R> IntoFn<A, Fun, P, R, Fut, ReqFn<'static, A, P, R, RefUnsafe>, AsyncReqFn2Id>
    for ()
where
    Fun: Fn(&'a mut A, P, Request<R>) -> Fut,
    Fut: Future<Output = MsgFlow<A>> + 'static + Send,
    A: Actor,
    P: Send + 'static,
    R: Send + 'static,
{
    unsafe fn into_fn(ptr: usize, _: Fun) -> ReqFn<'static, A, P, R, RefUnsafe> {
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
