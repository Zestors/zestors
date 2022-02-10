use std::{intrinsics::transmute, marker::PhantomData};

use futures::Future;

use crate::{
    actor::Actor,
    address::Addressable,
    flows::{Flow, ReqFlow},
    messaging::{Msg, Req},
};

//--------------------------------------------------------------------------------------------------
//  Fun
//--------------------------------------------------------------------------------------------------

/// A macro used to create a [Fun] which is used to [Callable::call] an actor with.
#[macro_export]
macro_rules! fun {
    ($x:expr) => {
        unsafe { $crate::callable::Fun::new(($x) as usize, $x) }
    };
}

#[macro_export]
macro_rules! call {
    ($address:ident, |$actor:ident, $state:ident, $param:ident| $block:block) => {
        
    };
}

#[derive(Clone, Copy)]
pub struct Fun<F> {
    ptr: usize,
    f: PhantomData<F>,
}

impl<'a, F> Fun<F> {
    // Create a new [Fun] struct. This struct is only used to get both type-coercion (from F), as
    // well as the function pointer (F) working.
    pub unsafe fn new(ptr: usize, f: F) -> Self {
        Self {
            ptr,
            f: PhantomData,
        }
    }

    // unsafe fn test() {
    //     let res = Self::new(||{} as usize, ||{});
    // }
}

//--------------------------------------------------------------------------------------------------
//  Callable trait definition
//--------------------------------------------------------------------------------------------------
pub trait Callable<'a, 'b, I, F, P, R, G, X> {
    fn call(&'a self, function: F, params: P) -> X;
}

// These are all function traits, to be implemented by functions with that amount of arguments
pub(crate) trait FnTrait<I, A, P, R, G> {}

trait SendFut<T>: Future<Output = T> + Send + 'static {}
impl<T> SendFut<T> for T where T: Future<Output = T> + Send + 'static {}

//--------------------------------------------------------------------------------------------------
//  Msg
//--------------------------------------------------------------------------------------------------
pub struct MsgId;
pub(crate) type MsgFn<'b, A, P> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> Flow<A>;

impl<'a, A: Actor, P, F> FnTrait<MsgId, A, P, (), ()> for F where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> Flow<A>
{
}

impl<'a, 'b, F, P, A, S> Callable<'a, 'b, MsgId, F, P, (), (), Msg<'a, A, P>> for S
where
    A: Actor,
    F: FnTrait<MsgId, A, P, (), ()>,
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> Flow<A> + Copy,
    P: Send + 'static,
    S: Addressable<A>,
{
    fn call(&'a self, function: F, params: P) -> Msg<'a, A, P> {
        let func: MsgFn<'b, A, P> = |actor, state, params| {
            // function(actor, state, params)
            Flow::Ok
        };

        let x = func as usize;
        // self.raw_address().new_msg(func, params)
        todo!()
    }
}

// fn helper(function: fn())

//--------------------------------------------------------------------------------------------------
//  AsyncMsg
//--------------------------------------------------------------------------------------------------

pub struct AsyncMsgId;
pub(crate) type AsyncMsgFn<'b, A, P, F> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> F;

impl<'a, A: Actor, P, F, G> FnTrait<AsyncMsgId, A, P, (), G> for F
where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> G,
    G: Future<Output = Flow<A>>,
{
}

impl<'a, 'b, F, P, A, G, S> Callable<'a, 'b, AsyncMsgId, F, P, (), G, Msg<'a, A, P>> for S
where
    A: Actor,
    F: FnTrait<AsyncMsgId, A, P, (), G>,
    F: Fn(&'b mut A, &'b mut <A as Actor>::State, P) -> G,
    G: Future<Output = Flow<A>> + Send + 'static,
    P: Send + 'static,
    S: Addressable<A>,
{
    fn call(&'a self, function: F, params: P) -> Msg<'a, A, P> {
        todo!()
        // let func: AsyncMsgFn<'b, A, P, G> = unsafe { transmute(function.ptr) };
        // self.raw_address().new_msg_async::<P, G>(func, params)
    }
}

//--------------------------------------------------------------------------------------------------
//  Req
//--------------------------------------------------------------------------------------------------

pub struct ReqId;
pub(crate) type ReqFn<'b, A, P, R> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> ReqFlow<A, R>;

impl<'a, A: Actor, P, F, R> FnTrait<ReqId, A, P, R, ()> for F where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> ReqFlow<A, R>
{
}

impl<'a, 'b, F, P, A, R, S> Callable<'a, 'b, ReqId, F, P, R, (), Req<'a, A, P, R>> for S
where
    A: Actor,
    F: FnTrait<ReqId, A, P, R, ()>,
    F: Fn(&'b mut A, &'b mut <A as Actor>::State, P) -> ReqFlow<A, R>,
    P: Send + 'static,
    R: Send + 'static,
    S: Addressable<A>,
{
    fn call(&'a self, function: F, params: P) -> Req<'a, A, P, R> {
        todo!()
        // let func: ReqFn<'b, A, P, R> = unsafe { transmute(function.ptr) };
        // self.raw_address().new_req(func, params)
    }
}

//--------------------------------------------------------------------------------------------------
//  AsyncReq
//--------------------------------------------------------------------------------------------------

pub struct AsyncReqId;
pub(crate) type AsyncReqFn<'b, A, P, F> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> F;

impl<'a, A: Actor, P, F, G, R> FnTrait<AsyncReqId, A, P, R, G> for F
where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> G,
    G: Future<Output = ReqFlow<A, R>>,
{
}

impl<'a, 'b, F, P, A, G, R, S> Callable<'a, 'b, AsyncReqId, F, P, R, G, Req<'a, A, P, R>> for S
where
    A: Actor,
    F: FnTrait<AsyncReqId, A, P, R, G>,
    F: Fn(&'b mut A, &'b mut <A as Actor>::State, P) -> G,
    G: Future<Output = ReqFlow<A, R>> + Send + 'static,
    P: Send + 'static,
    R: Send + 'static,
    S: Addressable<A>,
{
    fn call(&'a self, function: F, params: P) -> Req<'a, A, P, R> {
        todo!()
        // let func: AsyncReqFn<'b, A, P, G> = unsafe { transmute(function.ptr) };
        // self.raw_address().new_req_async::<P, R, G>(func, params)
    }
}
