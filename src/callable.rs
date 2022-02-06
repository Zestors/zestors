use std::{
    any::Any, fmt::Debug, intrinsics::transmute, marker::PhantomData, mem::transmute_copy, pin::Pin,
};

use futures::{Future, FutureExt};

use crate::{
    actor::{Actor, IsUnbounded, Unbounded},
    address::RawAddress,
    flows::{MsgFlow, ReqFlow},
    messaging::{Msg, Reply, Req},
};



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

// These are all function traits, to be implemented by functions with that amount of arguments
pub(crate) trait FnTrait<I, A, P1, R, G> {}

// These are the unique identifiers of every single function type supported.
// T should be one of the arguments that follow.
pub struct MsgID<T>(T);
pub struct MsgAsyncID<T>(T);
pub struct ReqID<T>(T);
pub struct ReqAsyncID<T>(T);
pub struct MsgIDNoState<T>(T);
pub struct MsgAsyncIDNoState<T>(T);
pub struct ReqIDNoState<T>(T);
pub struct ReqAsyncIDNoState<T>(T);

pub(crate) type MsgFnType1<'b, A, P1> =
    fn(&'b mut A, &'b mut <A as Actor>::State, P1) -> MsgFlow<A>;

pub(crate) type MsgFnTypeNostate1<'b, A, P1> = fn(&'b mut A, P1) -> MsgFlow<A>;


//-------------------------------------
// A: Msg
//-------------------------------------

impl<'a, A: Actor, P, F> FnTrait<MsgID<()>, A, P, (), ()> for F where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> MsgFlow<A>
{
}

impl<'a, 'b, F, P, A, S> Callable<'a, 'b, MsgID<()>, F, P, (), (), Msg<'a, A, P>> for S
where
    A: Actor,
    F: FnTrait<MsgID<()>, A, P, (), ()>,
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> MsgFlow<A>,
    P: Send + 'static,
    S: RawAddress<A>,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Msg<'a, A, P> {
        let func: MsgFnType1<'b, A, P> = unsafe { transmute(function.ptr) };
        self.raw_address().new_msg(func, params)
    }
}

//-------------------------------------
// B: MsgAsync
//-------------------------------------

pub(crate) type MsgAsyncFnType<'b, A, P, F> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> F;

impl<'a, A: Actor, P, F, G> FnTrait<MsgAsyncID<()>, A, P, (), G> for F
where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> G,
    G: Future<Output = MsgFlow<A>>,
{
}

impl<'a, 'b, F, P, A, G, S> Callable<'a, 'b, MsgAsyncID<()>, F, P, (), G, Msg<'a, A, P>> for S
where
    A: Actor,
    F: FnTrait<MsgAsyncID<()>, A, P, (), G>,
    F: Fn(&'b mut A, &'b mut <A as Actor>::State, P) -> G,
    G: Future<Output = MsgFlow<A>> + Send + 'static,
    P: Send + 'static,
    S: RawAddress<A>,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Msg<'a, A, P> {
        let func: MsgAsyncFnType<'b, A, P, G> = unsafe { transmute(function.ptr) };
        self.raw_address().new_msg_async::<P, G>(func, params)
    }
}

//-------------------------------------
// C: Req
//-------------------------------------

pub(crate) type ReqFnType<'b, A, P, R> =
    fn(&'b mut A, &'b mut <A as Actor>::State, P) -> ReqFlow<A, R>;

impl<'a, A: Actor, P, F, R> FnTrait<ReqID<()>, A, P, R, ()> for F where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> ReqFlow<A, R>
{
}

impl<'a, 'b, F, P, A, R, S> Callable<'a, 'b, ReqID<()>, F, P, R, (), Req<'a, A, P, R>> for S
where
    A: Actor,
    F: FnTrait<ReqID<()>, A, P, R, ()>,
    F: Fn(&'b mut A, &'b mut <A as Actor>::State, P) -> ReqFlow<A, R>,
    P: Send + 'static,
    R: Send + 'static,
    S: RawAddress<A>,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Req<'a, A, P, R> {
        let func: ReqFnType<'b, A, P, R> = unsafe { transmute(function.ptr) };
        self.raw_address().new_req(func, params)
    }
}

//-------------------------------------
// D: ReqAsync
//-------------------------------------

pub(crate) type ReqAsyncFnType<'b, A, P, F> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> F;

impl<'a, A: Actor, P, F, G, R> FnTrait<ReqAsyncID<()>, A, P, R, G> for F
where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> G,
    G: Future<Output = ReqFlow<A, R>>,
{
}

impl<'a, 'b, F, P, A, G, R, S> Callable<'a, 'b, ReqAsyncID<()>, F, P, R, G, Req<'a, A, P, R>>
    for S
where
    A: Actor,
    F: FnTrait<ReqAsyncID<()>, A, P, R, G>,
    F: Fn(&'b mut A, &'b mut <A as Actor>::State, P) -> G,
    G: Future<Output = ReqFlow<A, R>> + Send + 'static,
    P: Send + 'static,
    R: Send + 'static,
    S: RawAddress<A>,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Req<'a, A, P, R> {
        let func: ReqAsyncFnType<'b, A, P, G> = unsafe { transmute(function.ptr) };
        self.raw_address().new_req_async::<P, R, G>(func, params)
    }
}

//-------------------------------------
// E: MsgNostate
//-------------------------------------

impl<'a, A: Actor, P, F> FnTrait<MsgIDNoState<()>, A, P, (), ()> for F where
    F: Fn(&'a mut A, P) -> MsgFlow<A>
{
}

impl<'a, 'b, F, P, A, S> Callable<'a, 'b, MsgIDNoState<()>, F, P, (), (), Msg<'a, A, P>> for S
where
    A: Actor,
    F: FnTrait<MsgIDNoState<()>, A, P, (), ()>,
    F: Fn(&'b mut A, P) -> MsgFlow<A>,
    P: Send + 'static,
    S: RawAddress<A>,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Msg<'a, A, P> {
        let func: MsgFnTypeNostate1<'b, A, P> = unsafe { transmute(function.ptr) };
        self.raw_address().new_msg_nostate(func, params)
    }
}

//-------------------------------------
// F: MsgAsyncNostate
//-------------------------------------

pub(crate) type MsgAsyncFnNostateType<'b, A, P, F> = fn(&'b mut A, P) -> F;

impl<'a, A: Actor, P, F, G> FnTrait<MsgAsyncIDNoState<()>, A, P, (), G> for F
where
    F: Fn(&'a mut A, P) -> G,
    G: Future<Output = MsgFlow<A>>,
{
}

impl<'a, 'b, F, P, A, G, S> Callable<'a, 'b, MsgAsyncIDNoState<()>, F, P, (), G, Msg<'a, A, P>>
    for S
where
    A: Actor,
    F: FnTrait<MsgAsyncIDNoState<()>, A, P, (), G>,
    F: Fn(&'b mut A, P) -> G,
    G: Future<Output = MsgFlow<A>> + Send + 'static,
    P: Send + 'static,
    S: RawAddress<A>,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Msg<'a, A, P> {
        let func: MsgAsyncFnNostateType<'b, A, P, G> = unsafe { transmute(function.ptr) };
        self.raw_address()
            .new_msg_async_nostate::<P, G>(func, params)
    }
}

//-------------------------------------
// G: ReqNostate
//-------------------------------------

pub(crate) type ReqFnNostateType<'b, A, P, R> = fn(&'b mut A, P) -> ReqFlow<A, R>;

impl<'a, A: Actor, P, F, R> FnTrait<ReqIDNoState<()>, A, P, R, ()> for F where
    F: Fn(&'a mut A, P) -> ReqFlow<A, R>
{
}

impl<'a, 'b, F, P, A, R, S> Callable<'a, 'b, ReqIDNoState<()>, F, P, R, (), Req<'a, A, P, R>>
    for S
where
    A: Actor,
    F: FnTrait<ReqIDNoState<()>, A, P, R, ()>,
    F: Fn(&'b mut A, P) -> ReqFlow<A, R>,
    P: Send + 'static,
    R: Send + 'static,
    S: RawAddress<A>,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Req<'a, A, P, R> {
        let func: ReqFnNostateType<'b, A, P, R> = unsafe { transmute(function.ptr) };
        self.raw_address().new_req_nostate(func, params)
    }
}

//-------------------------------------
// H: ReqAsyncNostate
//-------------------------------------

pub(crate) type ReqAsyncFnNostateType<'b, A, P, F> = fn(&'b mut A, P) -> F;

impl<'a, A: Actor, P, F, G, R> FnTrait<ReqAsyncIDNoState<()>, A, P, R, G> for F
where
    F: Fn(&'a mut A, P) -> G,
    G: Future<Output = ReqFlow<A, R>>,
{
}

impl<'a, 'b, F, P, A, G, R, S>
    Callable<'a, 'b, ReqAsyncIDNoState<()>, F, P, R, G, Req<'a, A, P, R>> for S
where
    A: Actor,
    F: FnTrait<ReqAsyncIDNoState<()>, A, P, R, G>,
    F: Fn(&'b mut A, P) -> G,
    G: Future<Output = ReqFlow<A, R>> + Send + 'static,
    P: Send + 'static,
    R: Send + 'static,
    S: RawAddress<A>,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Req<'a, A, P, R> {
        let func: ReqAsyncFnNostateType<'b, A, P, G> = unsafe { transmute(function.ptr) };
        self.raw_address()
            .new_req_async_nostate::<P, R, G>(func, params)
    }
}
