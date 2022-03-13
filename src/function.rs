use crate::{prelude::*, flows::EventFlow};
use futures::{Future, FutureExt};
use serde::{Deserialize, Serialize};
use std::{any::Any, marker::PhantomData, mem::transmute, pin::Pin};

//------------------------------------------------------------------------------------------------
//  Function types
//------------------------------------------------------------------------------------------------

pub struct MsgFnId;
type MsgFnType<'a, A, P> = fn(&'a mut A, P) -> MsgFlow<A>;
pub struct AsyncMsgFnId;
type AsyncMsgFnType<'a, A, P, F> = fn(&'a mut A, P) -> F;
pub struct ReqFnId;
type ReqFnType<'a, A, P, R> = fn(&'a mut A, P) -> ReqFlow<A, R>;
pub struct AsyncReqFnId;
type AsyncReqFnType<'a, A, P, F> = fn(&'a mut A, P) -> F;
pub struct ReqFnIndirectId;
#[allow(dead_code)]
type ReqFnIndirectType<'a, A, P, R> = fn(&'a mut A, P, Request<R>) -> MsgFlow<A>;
pub struct AsyncReqFnIndirectId;
#[allow(dead_code)]
type AsyncReqFnIndirectType<'a, A, P, R, F> = fn(&'a mut A, P, Request<R>) -> F;
type HandlerFnType<A> = fn(&mut A, Params, usize) -> EventFlow<A>;
type AsyncHandlerFnType<'a, A> =
    fn(&mut A, Params, usize) -> Pin<Box<dyn Future<Output = EventFlow<A>> + Send + 'a>>;

//------------------------------------------------------------------------------------------------
//  Fn!
//------------------------------------------------------------------------------------------------

/// This macro creates either a [MsgFn] or a [ReqFn] from a normal `Fn`. The resulting function
/// can then be used to send messages or requests to processes, or to create [Action]s.
/// 
/// 
/// ## Usage
/// ```ignore
/// use zestors::prelude::*;
/// 
/// impl MyActor {
///     fn echo(&mut self, msg: String) -> ReqFlow<Self, String> {
///         ReqFlow::Reply(msg)
///     }
///     fn add(&mut self, msg: u32) -> MsgFlow<Self> {
///         self.val += msg;
///         MsgFlow::Ok
///     }
/// }
/// 
/// fn test() {
///     let function: ReqFn<MyActor, Fn(String) -> String> = Fn!(MyActor::echo);
///     let function: MsgFn<MyActor, Fn(u32)> = Fn!(MyActor::add_one);
/// }
/// ```
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
//  MsgFn
//------------------------------------------------------------------------------------------------

/// This represents a valid message function that can be sent to process of type `A`. value
/// `F` contains which parameters should be sent.
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

    fn __sync_handler__(actor: &mut A, params: Params, actual_fn: usize) -> EventFlow<A> {
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
        params: Params,
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

/// This represents a valid request function that can be sent to process of type `A`. value
/// `F` contains which parameters should be sent, and what will be returned as the reply.
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

    fn __sync_handler__(actor: &mut A, params: Params, actual_fn: usize) -> EventFlow<A> {
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
        params: Params,
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

    pub fn new_sync_indirect(function: fn(&'a mut A, P, Request<R>) -> MsgFlow<A>) -> Self
    where
        A: 'static,
        P: 'static,
    {
        Self {
            function: function as usize,
            handler: HandlerPtr::new_sync(Self::__sync_handler_indirect__),
            p: PhantomData,
        }
    }

    fn __sync_handler_indirect__(
        actor: &mut A,
        params: Params,
        actual_fn: usize,
    ) -> EventFlow<A>
    where
        A: 'static,
        P: 'static,
    {
        let function: fn(&mut A, P, Request<R>) -> MsgFlow<A> =
            unsafe { std::mem::transmute(actual_fn) };
        let (params, request) = unsafe { params.transmute_req() };
        function(actor, params, request).into_event_flow()
    }

    pub fn new_async_indirect<F>(function: fn(&'a mut A, P, Request<R>) -> F) -> Self
    where
        A: 'static,
        P: 'static,
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        Self {
            function: function as usize,
            handler: HandlerPtr::new_async(Self::__async_handler_indirect__::<F>),
            p: PhantomData,
        }
    }

    fn __async_handler_indirect__<F>(
        actor: &mut A,
        params: Params,
        actual_fn: usize,
    ) -> Pin<Box<dyn Future<Output = EventFlow<A>> + Send + 'static>>
    where
        A: 'static,
        P: 'static,
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        let function: fn(&mut A, P, Request<R>) -> F = unsafe { std::mem::transmute(actual_fn) };
        let (params, request) = unsafe { params.transmute_req() };
        Box::pin(function(actor, params, request).map(|val| val.into_event_flow()))
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
        params: Params,
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
//  HandlerParams
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) enum Params {
    Box(Box<dyn Any + Send>),
    Ref(NonStaticParams<'static>),
}

pub(crate) struct NonStaticParams<'a>(Box<dyn Send + 'a>);

impl<'a> NonStaticParams<'a> {
    pub fn new<P: Send + 'a, R: Send + 'a>(params: P, request: Request<R>) -> Self {
        Self(Box::new((params, request)))
    }

    pub unsafe fn transmute<P: Send + 'a, R: Send + 'a>(self) -> (P, Request<R>) {
        let (address, _metadata) = Box::into_raw(self.0).to_raw_parts();
        let boxed: Box<(P, Request<R>)> = transmute(address);
        *boxed
    }
}

unsafe impl Send for Params {}

impl Params {
    pub fn new_msg<P: Send + 'static>(params: P) -> Self {
        Self::Box(Box::new(params))
    }

    pub fn new_req<'a, P: Send + 'a, R: Send + 'a>(params: P, request: Request<R>) -> Self {
        let params = NonStaticParams::new(params, request);
        let params: NonStaticParams<'static> = unsafe { std::mem::transmute(params) };
        Self::Ref(params)
    }

    pub(crate) unsafe fn new_raw(inner: Box<dyn Any + Send>) -> Self {
        Self::Box(inner)
    }

    pub unsafe fn transmute_req<'a, P: Send + 'a, R: Send + 'a>(self) -> (P, Request<R>) {
        match self {
            Params::Box(boxed) => panic!(),
            Params::Ref(boxed) => boxed.transmute(),
        }
    }

    pub fn downcast_msg<P: Send + 'static>(self) -> Result<P, Box<dyn Any + Send>> {
        match self {
            Params::Box(boxed) => boxed.downcast().map(|p| *p),
            Params::Ref(_) => panic!(),
        }
    }
}

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
    Fut: Future<Output = MsgFlow<A>> + 'static + IsMsgFlow + Send,
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

impl<'params, A, Fun, P, Fut, R> IntoFn<A, Fun, P, R, Fut, ReqFn<A, fn(P) -> R>, AsyncReqFnId>
    for ()
where
    Fun: Fn(&'params mut A, P) -> Fut,
    Fut: Future<Output = ReqFlow<A, R>> + 'params + IsReqFlow + Send,
    A: Actor,
    P: Send + 'params,
    R: Send + 'params,
{
    unsafe fn into_fn(ptr: usize, _: Fun) -> ReqFn<A, fn(P) -> R> {
        let function: AsyncReqFnType<A, P, Fut> = transmute(ptr);
        ReqFn::new_async(function)
    }
}

impl<A, F, P, R> IntoFn<A, F, P, R, (), ReqFn<A, fn(P) -> R>, ReqFnIndirectId> for ()
where
    F: Fn(&mut A, P, Request<R>) -> MsgFlow<A>,
    A: Actor,
    P: Send + 'static,
    R: Send + 'static,
{
    unsafe fn into_fn(ptr: usize, _: F) -> ReqFn<A, fn(P) -> R> {
        let function: fn(&mut A, P, Request<R>) -> MsgFlow<A> = transmute(ptr);
        ReqFn::new_sync_indirect(function)
    }
}

impl<'a, A, Fun, P, Fut, R> IntoFn<A, Fun, P, R, Fut, ReqFn<A, fn(P) -> R>, AsyncReqFnIndirectId>
    for ()
where
    Fun: Fn(&'a mut A, P, Request<R>) -> Fut,
    Fut: Future<Output = MsgFlow<A>> + 'static + IsMsgFlow + Send,
    A: Actor,
    P: Send + 'static,
    R: Send + 'static,
{
    unsafe fn into_fn(ptr: usize, _: Fun) -> ReqFn<A, fn(P) -> R> {
        let function: fn(&mut A, P, Request<R>) -> Fut = transmute(ptr);
        ReqFn::new_async_indirect(function)
    }
}

//------------------------------------------------------------------------------------------------
//  MsgHelperTrait
//------------------------------------------------------------------------------------------------

pub trait IsMsgFlow {}
pub trait IsReqFlow {}

impl<A, T> IsMsgFlow for T
where
    A: Actor,
    T: Future<Output = MsgFlow<A>>,
{
}

impl<A, R, T> IsReqFlow for T
where
    A: Actor,
    T: Future<Output = ReqFlow<A, R>>,
{
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
impl<'a> std::fmt::Debug for NonStaticParams<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RefParams").field(&"").finish()
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

//--------------------------------------------------------------------------------------------------
//  test
//--------------------------------------------------------------------------------------------------

mod test {
    use crate::{prelude::*, test::TestActor};

    #[tokio::test]
    pub async fn test_all_functions_basic() {
        let child = actor::spawn::<TestActor>(10);
        let addr = child.addr();

        let res1 = addr.msg(Fn!(TestActor::sync_msg), "hi").unwrap();
        let res2 = addr.msg(Fn!(TestActor::async_msg), "hi").unwrap();
        let res3 = addr
            .req(Fn!(TestActor::sync_req), "hi")
            .unwrap()
            .await
            .unwrap();
        let res4 = addr
            .req(Fn!(TestActor::async_req), "hi")
            .unwrap()
            .await
            .unwrap();
        let res5 = addr
            .req(Fn!(TestActor::sync_req_indirect), "hi")
            .unwrap()
            .await
            .unwrap();
        let res6 = addr
            .req(Fn!(TestActor::async_req_indirect), "hi")
            .unwrap()
            .await
            .unwrap();

        assert_eq!(
            (res1, res2, res3, res4, res5, res6),
            ((), (), "hi", "hi", "hi", "hi")
        );
    }

    #[allow(dead_code)]
    impl TestActor {
        fn sync_msg(&mut self, _str: &str) -> MsgFlow<Self> {
            MsgFlow::Ok
        }

        async fn async_msg(&mut self, _str: &str) -> MsgFlow<Self> {
            MsgFlow::Ok
        }

        fn sync_req<'a>(&mut self, str: &'a str) -> ReqFlow<Self, &'a str> {
            ReqFlow::Reply(str)
        }

        async fn async_req<'a>(&mut self, str: &'a str) -> ReqFlow<Self, &'a str> {
            ReqFlow::Reply(str)
        }

        fn sync_req_indirect<'a>(
            &mut self,
            str: &'a str,
            request: Request<&'a str>,
        ) -> MsgFlow<Self> {
            request.reply(str).unwrap();
            MsgFlow::Ok
        }

        async fn async_req_indirect<'a>(
            &mut self,
            str: &'a str,
            request: Request<&'a str>,
        ) -> MsgFlow<Self> {
            request.reply(str).unwrap();
            MsgFlow::Ok
        }
    }
}
