use std::{
    any::Any, fmt::Debug, intrinsics::transmute, marker::PhantomData, mem::transmute_copy, pin::Pin,
};

use futures::{Future, FutureExt};

use crate::{
    actor::Actor,
    address::Address,
    flow::{MsgFlow, ReqFlow},
    messaging::{InnerRequest, Msg, PacketSender, Reply, Req, SendError, TrySendError},
    packets::{HandlerFn, HandlerFnAsync, HandlerPacket, Packet},
};

//-------------------------------------
// Sendable
//-------------------------------------

pub trait Sendable<'a, 'b, I, F, P, R, G, A> {
    fn try_send(&'a self, function: RemoteFunction<F>, params: P) -> Result<R, TrySendError<P>>;

    fn send(
        &'a self,
        function: RemoteFunction<F>,
        params: P,
    ) -> Pin<Box<dyn Future<Output = Result<R, SendError<P>>> + 'a>>;
}

impl<'a, 'b, I, F, P, G, A, T> Sendable<'a, 'b, I, F, P, (), G, A> for T
where
    T: Callable<'a, 'b, I, F, P, (), G, Msg<'a, A, P>>,
    A: Actor,
    P: Send + 'static,
    F: 'a
{
    fn try_send(&'a self, function: RemoteFunction<F>, params: P) -> Result<(), TrySendError<P>> {
        self.call(function, params).try_send()
    }

    fn send(
        &'a self,
        function: RemoteFunction<F>,
        params: P,
    ) -> Pin<Box<dyn Future<Output = Result<(), SendError<P>>> + 'a>> {
        Box::pin(async move { self.call(function, params).send().await })
    }
}

impl<'a, 'b, I, F, P, G, R, A, T> Sendable<'a, 'b, I, F, P, Reply<R>, G, A> for T
where
    T: Callable<'a, 'b, I, F, P, R, G, Req<'a, A, P, R>>,
    A: Actor,
    P: Send + 'static,
    R: Send + 'static,
    F: 'a
{
    fn try_send(
        &'a self,
        function: RemoteFunction<F>,
        params: P,
    ) -> Result<Reply<R>, TrySendError<P>> {
        self.call(function, params).try_send()
    }

    fn send(
        &'a self,
        function: RemoteFunction<F>,
        params: P,
    ) -> Pin<Box<dyn Future<Output = Result<Reply<R>, SendError<P>>> + 'a>> {
        Box::pin(async move { self.call(function, params).send().await })
    }
}

//-------------------------------------
// Macro
//-------------------------------------

#[macro_export]
macro_rules! func {
    ($x:expr) => {
        unsafe { $crate::callable::RemoteFunction::new(($x) as usize, $x) }
    };
}

//-------------------------------------
// RemoteFunction
//-------------------------------------

#[derive(Clone, Copy)]
pub struct RemoteFunction<F> {
    ptr: usize,
    f: PhantomData<F>,
}

impl<'a, F> RemoteFunction<F> {
    pub unsafe fn new(ptr: usize, f: F) -> Self {
        Self {
            ptr,
            f: PhantomData,
        }
    }
}

//-------------------------------------
// Callable
//-------------------------------------

pub trait Callable<'a, 'b, I, F, P, R, G, X> {
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> X;
}

//-------------------------------------
// FnTrait
//-------------------------------------

pub(crate) trait FnTrait<I, A, P, R, G> {}

//-------------------------------------
// A: Msg
//-------------------------------------

pub struct MsgIdent;
pub(crate) type MsgFnType<'b, A, P> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> MsgFlow<A>;

impl<'a, A: Actor, P, F> FnTrait<MsgIdent, A, P, (), ()> for F where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> MsgFlow<A>
{
}

impl<'a, 'b, F, P, A> Callable<'a, 'b, MsgIdent, F, P, (), (), Msg<'a, A, P>> for Address<A>
where
    A: Actor,
    F: FnTrait<MsgIdent, A, P, (), ()>,
    P: Send + 'static,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Msg<'a, A, P> {
        self.msg(unsafe { transmute(function.ptr) }, params)
    }
}

//-------------------------------------
// B: MsgAsync
//-------------------------------------

pub struct MsgAsyncIdent;
pub(crate) type MsgAsyncFnType<'b, A, P, F> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> F;

impl<'a, A: Actor, P, F, G> FnTrait<MsgAsyncIdent, A, P, (), G> for F
where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> G,
    G: Future<Output = MsgFlow<A>>,
{
}

impl<'a, 'b, F, P, A, G> Callable<'a, 'b, MsgAsyncIdent, F, P, (), G, Msg<'a, A, P>> for Address<A>
where
    A: Actor,
    F: FnTrait<MsgAsyncIdent, A, P, (), G>,
    F: Fn(&'b mut A, &'b mut <A as Actor>::State, P) -> G,
    G: Future<Output = MsgFlow<A>> + Send + 'static,
    P: Send + 'static,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Msg<'a, A, P> {
        self.msg_async::<P, G>(unsafe { transmute(function.ptr) }, params)
    }
}

//-------------------------------------
// C: Req
//-------------------------------------

pub struct ReqIdent;
pub(crate) type ReqFnType<'b, A, P, R> =
    fn(&'b mut A, &'b mut <A as Actor>::State, P) -> ReqFlow<A, R>;

impl<'a, A: Actor, P, F, R> FnTrait<ReqIdent, A, P, R, ()> for F where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> ReqFlow<A, R>
{
}

impl<'a, 'b, F, P, A, R> Callable<'a, 'b, ReqIdent, F, P, R, (), Req<'a, A, P, R>> for Address<A>
where
    A: Actor,
    F: FnTrait<ReqIdent, A, P, R, ()>,
    F: Fn(&'b mut A, &'b mut <A as Actor>::State, P) -> ReqFlow<A, R>,
    P: Send + 'static,
    R: Send + 'static,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Req<'a, A, P, R> {
        self.req(unsafe { transmute(function.ptr) }, params)
    }
}

//-------------------------------------
// D: ReqAsync
//-------------------------------------

pub struct ReqAsyncIdent;
pub(crate) type ReqAsyncFnType<'b, A, P, F> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> F;

impl<'a, A: Actor, P, F, G, R> FnTrait<ReqAsyncIdent, A, P, R, G> for F
where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> G,
    G: Future<Output = ReqFlow<A, R>>,
{
}

impl<'a, 'b, F, P, A, G, R> Callable<'a, 'b, ReqAsyncIdent, F, P, R, G, Req<'a, A, P, R>>
    for Address<A>
where
    A: Actor,
    F: FnTrait<ReqAsyncIdent, A, P, R, G>,
    F: Fn(&'b mut A, &'b mut <A as Actor>::State, P) -> G,
    G: Future<Output = ReqFlow<A, R>> + Send + 'static,
    P: Send + 'static,
    R: Send + 'static,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Req<'a, A, P, R> {
        self.req_async::<P, R, G>(unsafe { transmute(function.ptr) }, params)
    }
}

//-------------------------------------
// E: MsgNostate
//-------------------------------------

pub struct MsgIdentNoState;
pub(crate) type MsgFnNostateType<'b, A, P> = fn(&'b mut A, P) -> MsgFlow<A>;

impl<'a, A: Actor, P, F> FnTrait<MsgIdentNoState, A, P, (), ()> for F where
    F: Fn(&'a mut A, P) -> MsgFlow<A>
{
}

impl<'a, 'b, F, P, A> Callable<'a, 'b, MsgIdentNoState, F, P, (), (), Msg<'a, A, P>> for Address<A>
where
    A: Actor,
    F: FnTrait<MsgIdentNoState, A, P, (), ()>,
    F: Fn(&'b mut A, P) -> MsgFlow<A>,
    P: Send + 'static,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Msg<'a, A, P> {
        self.msg(unsafe { transmute(function.ptr) }, params)
    }
}

//-------------------------------------
// F: MsgAsyncNostate
//-------------------------------------

pub struct MsgAsyncIdentNoState;
pub(crate) type MsgAsyncFnNostateType<'b, A, P, F> = fn(&'b mut A, P) -> F;

impl<'a, A: Actor, P, F, G> FnTrait<MsgAsyncIdentNoState, A, P, (), G> for F
where
    F: Fn(&'a mut A, P) -> G,
    G: Future<Output = MsgFlow<A>>,
{
}

impl<'a, 'b, F, P, A, G> Callable<'a, 'b, MsgAsyncIdentNoState, F, P, (), G, Msg<'a, A, P>>
    for Address<A>
where
    A: Actor,
    F: FnTrait<MsgAsyncIdentNoState, A, P, (), G>,
    F: Fn(&'b mut A, P) -> G,
    G: Future<Output = MsgFlow<A>> + Send + 'static,
    P: Send + 'static,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Msg<'a, A, P> {
        self.msg_async::<P, G>(unsafe { transmute(function.ptr) }, params)
    }
}

//-------------------------------------
// G: ReqNostate
//-------------------------------------

pub struct ReqIdentNoState;
pub(crate) type ReqFnNostateType<'b, A, P, R> = fn(&'b mut A, P) -> ReqFlow<A, R>;

impl<'a, A: Actor, P, F, R> FnTrait<ReqIdentNoState, A, P, R, ()> for F where
    F: Fn(&'a mut A, P) -> ReqFlow<A, R>
{
}

impl<'a, 'b, F, P, A, R> Callable<'a, 'b, ReqIdentNoState, F, P, R, (), Req<'a, A, P, R>>
    for Address<A>
where
    A: Actor,
    F: FnTrait<ReqIdentNoState, A, P, R, ()>,
    F: Fn(&'b mut A, P) -> ReqFlow<A, R>,
    P: Send + 'static,
    R: Send + 'static,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Req<'a, A, P, R> {
        self.req(unsafe { transmute(function.ptr) }, params)
    }
}

//-------------------------------------
// H: ReqAsyncNostate
//-------------------------------------

pub struct ReqAsyncIdentNoState;
pub(crate) type ReqAsyncFnNostateType<'b, A, P, F> = fn(&'b mut A, P) -> F;

impl<'a, A: Actor, P, F, G, R> FnTrait<ReqAsyncIdentNoState, A, P, R, G> for F
where
    F: Fn(&'a mut A, P) -> G,
    G: Future<Output = ReqFlow<A, R>>,
{
}

impl<'a, 'b, F, P, A, G, R> Callable<'a, 'b, ReqAsyncIdentNoState, F, P, R, G, Req<'a, A, P, R>>
    for Address<A>
where
    A: Actor,
    F: FnTrait<ReqAsyncIdentNoState, A, P, R, G>,
    F: Fn(&'b mut A, P) -> G,
    G: Future<Output = ReqFlow<A, R>> + Send + 'static,
    P: Send + 'static,
    R: Send + 'static,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Req<'a, A, P, R> {
        self.req_async::<P, R, G>(unsafe { transmute(function.ptr) }, params)
    }
}

//-------------------------------------
// Callable trait definition
//-------------------------------------
