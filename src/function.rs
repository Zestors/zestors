use crate::{
    actor::Actor,
    distributed::node::NodeActor,
    flows::{EventFlow, MsgFlow, ReqFlow},
    messaging::{Reply, Request},
};
use futures::{Future, FutureExt};
use serde::{Deserialize, Serialize};
use std::{any::Any, marker::PhantomData, mem::transmute, pin::Pin};

//------------------------------------------------------------------------------------------------
//  Types
//------------------------------------------------------------------------------------------------

pub struct MsgFnId;
type MsgFnType<'a, A, P> = fn(&'a mut A, P) -> MsgFlow<A>;
pub struct AsyncMsgFnId;
type AsyncMsgFnType<'a, A, P, F> = fn(&'a mut A, P) -> F;
pub struct ReqFnId;
type ReqFnType<'a, A, P, R> = fn(&'a mut A, P) -> ReqFlow<A, R>;
pub struct AsyncReqFnId;
type AsyncReqFnType<'a, A, P, F> = fn(&'a mut A, P) -> F;

type HandlerFnType<A> = fn(&mut A, HandlerParams, usize) -> EventFlow<A>;
type AsyncHandlerFnType<'a, A> =
    fn(&mut A, HandlerParams, usize) -> Pin<Box<dyn Future<Output = EventFlow<A>> + Send + 'a>>;

//------------------------------------------------------------------------------------------------
//  MsgFn
//------------------------------------------------------------------------------------------------

pub struct MsgFn<A: ?Sized, F: ?Sized> {
    function: usize,
    handler: HandlerPtr,
    p: PhantomData<(PhantomData<*const A>, PhantomData<*const F>)>,
}

impl<'a, A: Actor, P: Send + 'static> MsgFn<A, fn(P)> {
    pub fn new_sync(function: fn(&'a mut A, P) -> MsgFlow<A>) -> Self {
        Self {
            function: function as usize,
            handler: HandlerPtr::new_sync(Self::__sync_handler__),
            p: PhantomData,
        }
    }

    fn __sync_handler__(actor: &mut A, params: HandlerParams, actual_fn: usize) -> EventFlow<A> {
        let function: fn(&mut A, P) -> MsgFlow<A> = unsafe { std::mem::transmute(actual_fn) };
        function(actor, params.downcast_msg().unwrap()).into_event_flow()
    }

    pub fn new_async<F: Future<Output = MsgFlow<A>> + 'static + Send>(
        function: fn(&'a mut A, P) -> F,
    ) -> Self {
        Self {
            function: function as usize,
            handler: HandlerPtr::new_async(Self::__async_handler__::<F>),
            p: PhantomData,
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
        let function: fn(&mut A, P) -> F = unsafe { std::mem::transmute(actual_fn) };
        Box::pin(function(actor, params.downcast_msg().unwrap()).map(|f| f.into_event_flow()))
    }
}

//------------------------------------------------------------------------------------------------
//  ReqFn
//------------------------------------------------------------------------------------------------

pub struct ReqFn<A: ?Sized, F: ?Sized> {
    function: usize,
    handler: HandlerPtr,
    p: PhantomData<(PhantomData<*const A>, PhantomData<*const F>)>,
}

impl<'a, A: Actor, P: Send + 'a, R: Send + 'a> ReqFn<A, fn(P) -> R> {
    pub fn new_sync(function: fn(&'a mut A, P) -> ReqFlow<A, R>) -> Self {
        Self {
            function: function as usize,
            handler: HandlerPtr::new_sync(Self::__sync_handler__),
            p: PhantomData,
        }
    }

    fn __sync_handler__(actor: &mut A, params: HandlerParams, actual_fn: usize) -> EventFlow<A> {
        let function: fn(&mut A, P) -> ReqFlow<A, R> = unsafe { std::mem::transmute(actual_fn) };
        let (params, request) = unsafe { params.transmute_req() };
        let (flow, reply) = function(actor, params).take_reply();
        if let Some(reply) = reply {
            let _ = request.reply(reply);
        }
        flow
    }

    pub fn new_async<F: Future<Output = ReqFlow<A, R>> + 'a + Send>(
        function: fn(&'a mut A, P) -> F,
    ) -> Self {
        Self {
            function: function as usize,
            handler: HandlerPtr::new_async(Self::__async_handler__::<F>),
            p: PhantomData,
        }
    }

    fn __async_handler__<F: Future<Output = ReqFlow<A, R>> + Send + 'a>(
        actor: &mut A,
        params: HandlerParams,
        actual_fn: usize,
    ) -> Pin<Box<dyn Future<Output = EventFlow<A>> + Send + 'a>> {
        let function: fn(&mut A, P) -> F = unsafe { std::mem::transmute(actual_fn) };
        let (params, request) = unsafe { params.transmute_req() };
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

impl<A, F> From<ReqFn<A, F>> for AnyFn {
    fn from(ptr: ReqFn<A, F>) -> Self {
        Self {
            function: ptr.function,
            handler: ptr.handler,
        }
    }
}

impl<A, F> From<MsgFn<A, F>> for AnyFn {
    fn from(ptr: MsgFn<A, F>) -> Self {
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
    pub fn new_msg<P: Send + 'static>(params: P) -> Self {
        Self::Box(Box::new(params))
    }

    pub fn new_req<'a, P: Send + 'a, R: Send + 'a>(params: P, request: Request<R>) -> Self {
        let params = RefParams::new(params, request);
        let params: RefParams<'static> = unsafe { std::mem::transmute(params) };
        Self::Ref(params)
    }

    pub(crate) unsafe fn new_raw(inner: Box<dyn Any + Send>) -> Self {
        Self::Box(inner)
    }

    pub unsafe fn transmute_req<'a, P: Send + 'a, R: Send + 'a>(self) -> (P, Request<R>) {
        match self {
            HandlerParams::Box(boxed) => panic!(),
            HandlerParams::Ref(boxed) => boxed.transmute(),
        }
    }

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
impl<A, F> Clone for MsgFn<A, F> {
    fn clone(&self) -> Self {
        Self {
            function: self.function.clone(),
            handler: self.handler.clone(),
            p: self.p.clone(),
        }
    }
}
impl<A, F> Clone for ReqFn<A, F> {
    fn clone(&self) -> Self {
        Self {
            function: self.function.clone(),
            handler: self.handler.clone(),
            p: self.p.clone(),
        }
    }
}
impl Copy for AnyFn {}
impl<A, F> Copy for MsgFn<A, F> {}
impl<A, F> Copy for ReqFn<A, F> {}
unsafe impl Send for AnyFn {}
unsafe impl Sync for AnyFn {}
unsafe impl<A, F> Send for ReqFn<A, F> {}
unsafe impl<A, F> Sync for ReqFn<A, F> {}
unsafe impl<A, F> Send for MsgFn<A, F> {}
unsafe impl<A, F> Sync for MsgFn<A, F> {}

//------------------------------------------------------------------------------------------------
//  IntoFn
//------------------------------------------------------------------------------------------------

pub trait IntoFn<A, Fun, P, R, Fut, X, Id> {
    unsafe fn into_fn(function_ptr: usize, function: Fun) -> X;
}

impl<'a, 'f, A, Fun, P> IntoFn<A, Fun, P, (), (), MsgFn<A, fn(P)>, MsgFnId> for ()
where
    Fun: Fn(&'a mut A, P) -> MsgFlow<A>,
    A: Actor,
    P: Send + 'static,
{
    unsafe fn into_fn(ptr: usize, _: Fun) -> MsgFn<A, fn(P)> {
        let function: MsgFnType<A, P> = transmute(ptr);
        MsgFn::new_sync(function)
    }
}

impl<'a, A, Fun, P, Fut> IntoFn<A, Fun, P, (), Fut, MsgFn<A, fn(P)>, AsyncMsgFnId> for ()
where
    Fun: Fn(&'a mut A, P) -> Fut,
    Fut: Future<Output = MsgFlow<A>> + 'static + MsgHelperTrait + Send,
    A: Actor,
    P: Send + 'static,
{
    unsafe fn into_fn(ptr: usize, _: Fun) -> MsgFn<A, fn(P)> {
        let function: AsyncMsgFnType<A, P, Fut> = transmute(ptr);
        MsgFn::new_async(function)
    }
}

impl<'params, A, F, P, R> IntoFn<A, F, P, R, (), ReqFn<A, fn(P) -> R>, ReqFnId> for ()
where
    F: Fn(&'params mut A, P) -> ReqFlow<A, R>,
    A: Actor,
    P: Send + 'params,
    R: Send + 'params,
{
    unsafe fn into_fn(ptr: usize, _: F) -> ReqFn<A, fn(P) -> R> {
        let function: ReqFnType<A, P, R> = transmute(ptr);
        ReqFn::new_sync(function)
    }
}

impl<'params, A, Fun, P, Fut, R> IntoFn<A, Fun, P, R, Fut, ReqFn<A, fn(P) -> R>, AsyncReqFnId> for ()
where
    Fun: Fn(&'params mut A, P) -> Fut,
    Fut: Future<Output = ReqFlow<A, R>> + 'params + ReqHelperTrait + Send,
    A: Actor,
    P: Send + 'params,
    R: Send + 'params,
{
    unsafe fn into_fn(ptr: usize, _: Fun) -> ReqFn<A, fn(P) -> R> {
        let function: AsyncReqFnType<A, P, Fut> = transmute(ptr);
        ReqFn::new_async(function)
    }
}

// //------------------------------------------------------------------------------------------------
// //  MsgHelperTrait
// //------------------------------------------------------------------------------------------------

pub trait MsgHelperTrait {}
pub trait ReqHelperTrait {}
impl<A: Actor, T: Future<Output = MsgFlow<A>>> MsgHelperTrait for T {}
impl<A: Actor, R, T: Future<Output = ReqFlow<A, R>>> ReqHelperTrait for T {}

//------------------------------------------------------------------------------------------------
//  Fn!
//------------------------------------------------------------------------------------------------

#[macro_export]
macro_rules! Fn {
    ($function:expr) => {
        #[allow(unused_unsafe)]
        unsafe {
            <() as $crate::function::IntoFn<_, _, _, _, _, _, _>>::into_fn(
                $function as usize,
                $function,
            )
        }
    };
}

//------------------------------------------------------------------------------------------------
//  Testing
//------------------------------------------------------------------------------------------------

fn req_fn<'a>(actor: &mut NodeActor, val: &'a str) -> ReqFlow<NodeActor, &'a str> {
    ReqFlow::Reply(val)
}

async fn async_req_fn<'a>(actor: &mut NodeActor, val: &'a str) -> ReqFlow<NodeActor, &'a str> {
    ReqFlow::Reply(val)
}

fn msg_fn<'a>(actor: &mut NodeActor, val: &'a str) -> MsgFlow<NodeActor> {
    MsgFlow::Ok
}

async fn async_msg_fn<'a>(actor: &mut NodeActor, val: &'a str) -> MsgFlow<NodeActor> {
    MsgFlow::Ok
}

fn test() {
    let fn1 = ReqFn::new_sync(req_fn);
    let fn2 = ReqFn::new_async(async_req_fn);
    let fn3 = MsgFn::new_sync(msg_fn);
    let fn4 = MsgFn::new_async(async_msg_fn);

    let val = "string".to_string();
    let res = other_test(fn1, &val);
    let res = other_test(fn2, &val);
    // drop(val);

    println!("{}", res);
}

fn other_test<A, P, R>(function: ReqFn<A, fn(P) -> R>, p: P) -> R {
    todo!()
}
