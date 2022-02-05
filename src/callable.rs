use std::{
    any::Any, fmt::Debug, intrinsics::transmute, marker::PhantomData, mem::transmute_copy, pin::Pin,
};

use futures::{Future, FutureExt};

use crate::{
    actor::{Actor, IsUnbounded, Unbounded},
    address::RawAddress,
    flow::{MsgFlow, ReqFlow},
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
// pub(crate) trait FnTrait0<I, A, R, G> {}
pub(crate) trait FnTrait1<I, A, P1, R, G> {}
// pub(crate) trait FnTrait2<I, A, P1, P2, R, G> {}
// pub(crate) trait FnTrait3<I, A, P1, P2, P3, R, G> {}
// pub(crate) trait FnTrait4<I, A, P1, P2, P3, P4, R, G> {}
// pub(crate) trait FnTrait5<I, A, P1, P2, P3, P4, P5, R, G> {}
// pub(crate) trait FnTrait6<I, A, P1, P2, P3, P4, P5, P6, R, G> {}
// pub(crate) trait FnTrait7<I, A, P1, P2, P3, P4, P5, P6, P7, R, G> {}
// pub(crate) trait FnTrait8<I, A, P1, P2, P3, P4, P5, P6, P7, P8, R, G> {}
// pub(crate) trait FnTrait9<I, A, P1, P2, P3, P4, P5, P6, P7, P8, P9, R, G> {}

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

// The amount of arguments that a function has
// pub struct ZeroArgs;
pub struct OneArg;
// pub struct TwoArgs;
// pub struct ThreeArgs;
// pub struct FourArgs;
// pub struct FiveArgs;
// pub struct SixArgs;
// pub struct SevenArgs;
// pub struct EigthArgs;
// pub struct NineArgs;

// pub(crate) type MsgFnType0<'b, A> = fn(&'b mut A, &'b mut <A as Actor>::State) -> MsgFlow<A>;
pub(crate) type MsgFnType1<'b, A, P1> =
    fn(&'b mut A, &'b mut <A as Actor>::State, P1) -> MsgFlow<A>;
// pub(crate) type MsgFnType2<'b, A, P1, P2> =
//     fn(&'b mut A, &'b mut <A as Actor>::State, P1, P2) -> MsgFlow<A>;
// pub(crate) type MsgFnType3<'b, A, P1, P2, P3> =
//     fn(&'b mut A, &'b mut <A as Actor>::State, P1, P2, P3) -> MsgFlow<A>;
// pub(crate) type MsgFnType4<'b, A, P1, P2, P3, P4> =
//     fn(&'b mut A, &'b mut <A as Actor>::State, P1, P2, P3, P4) -> MsgFlow<A>;
// pub(crate) type MsgFnType5<'b, A, P1, P2, P3, P4, P5> =
//     fn(&'b mut A, &'b mut <A as Actor>::State, P1, P2, P3, P4, P5) -> MsgFlow<A>;
// pub(crate) type MsgFnType6<'b, A, P1, P2, P3, P4, P5, P6> =
//     fn(&'b mut A, &'b mut <A as Actor>::State, P1, P2, P3, P4, P5, P6) -> MsgFlow<A>;
// pub(crate) type MsgFnType7<'b, A, P1, P2, P3, P4, P5, P6, P7> =
//     fn(&'b mut A, &'b mut <A as Actor>::State, P1, P2, P3, P4, P5, P6, P7) -> MsgFlow<A>;
// pub(crate) type MsgFnType8<'b, A, P1, P2, P3, P4, P5, P6, P7, P8> =
//     fn(&'b mut A, &'b mut <A as Actor>::State, P1, P2, P3, P4, P5, P6, P7, P8) -> MsgFlow<A>;
// pub(crate) type MsgFnType9<'b, A, P1, P2, P3, P4, P5, P6, P7, P8, P9> =
//     fn(&'b mut A, &'b mut <A as Actor>::State, P1, P2, P3, P4, P5, P6, P7, P8, P9) -> MsgFlow<A>;

// pub(crate) type MsgFnTypeNostate0<'b, A> = fn(&'b mut A, &'b mut <A as Actor>::State) -> MsgFlow<A>;
pub(crate) type MsgFnTypeNostate1<'b, A, P1> = fn(&'b mut A, P1) -> MsgFlow<A>;
// pub(crate) type MsgFnTypeNostate2<'b, A, P1, P2> = fn(&'b mut A, P1, P2) -> MsgFlow<A>;
// pub(crate) type MsgFnTypeNostate3<'b, A, P1, P2, P3> = fn(&'b mut A, P1, P2, P3) -> MsgFlow<A>;
// pub(crate) type MsgFnTypeNostate4<'b, A, P1, P2, P3, P4> =
//     fn(&'b mut A, P1, P2, P3, P4) -> MsgFlow<A>;
// pub(crate) type MsgFnTypeNostate5<'b, A, P1, P2, P3, P4, P5> =
//     fn(&'b mut A, P1, P2, P3, P4, P5) -> MsgFlow<A>;
// pub(crate) type MsgFnTypeNostate6<'b, A, P1, P2, P3, P4, P5, P6> =
//     fn(&'b mut A, P1, P2, P3, P4, P5, P6) -> MsgFlow<A>;
// pub(crate) type MsgFnTypeNostate7<'b, A, P1, P2, P3, P4, P5, P6, P7> =
//     fn(&'b mut A, P1, P2, P3, P4, P5, P6, P7) -> MsgFlow<A>;
// pub(crate) type MsgFnTypeNostate8<'b, A, P1, P2, P3, P4, P5, P6, P7, P8> =
//     fn(&'b mut A, P1, P2, P3, P4, P5, P6, P7, P8) -> MsgFlow<A>;
// pub(crate) type MsgFnTypeNostate9<'b, A, P1, P2, P3, P4, P5, P6, P7, P8, P9> =
//     fn(&'b mut A, P1, P2, P3, P4, P5, P6, P7, P8, P9) -> MsgFlow<A>;

//-------------------------------------
// A: Msg
//-------------------------------------

impl<'a, A: Actor, P, F> FnTrait1<MsgID<OneArg>, A, P, (), ()> for F where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> MsgFlow<A>
{
}

impl<'a, 'b, F, P, A, S> Callable<'a, 'b, MsgID<OneArg>, F, P, (), (), Msg<'a, A, P>> for S
where
    A: Actor,
    F: FnTrait1<MsgID<OneArg>, A, P, (), ()>,
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> MsgFlow<A>,
    P: Send + 'static,
    S: RawAddress<A>,
{
    fn call(&'a self, function: RemoteFunction<F>, params: P) -> Msg<'a, A, P> {
        let func: MsgFnType1<'b, A, P> = unsafe { transmute(function.ptr) };
        self.raw_address().new_msg(func, params)
    }
}

// impl<'a, A: Actor, P1, P2, F> FnTrait2<MsgIDNoState<TwoArgs>, A, P1, P2, (), ()> for F where
//     F: Fn(&'a mut A, P1, P2) -> MsgFlow<A>
// {
// }

// impl<'a, 'b, F, P1, P2, A, S>
//     Callable<'a, 'b, MsgIDNoState<TwoArgs>, F, (P1, P2), (), (), Msg<'a, A, (P1, P2)>> for S
// where
//     A: Actor,
//     F: FnTrait2<MsgIDNoState<TwoArgs>, A, P1, P2, (), ()>,
//     F: Fn(&'a mut A, P1, P2) -> MsgFlow<A>,
//     P1: Send + 'static,
//     P2: Send + 'static,
//     S: RawAddress<A>,
// {
//     fn call(&'a self, function: RemoteFunction<F>, params: (P1, P2)) -> Msg<'a, A, (P1, P2)> {
//         // self.address().msg(unsafe { transmute(function.ptr) }, params)
//         todo!()
//     }
// }

//-------------------------------------
// B: MsgAsync
//-------------------------------------

pub(crate) type MsgAsyncFnType<'b, A, P, F> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> F;

impl<'a, A: Actor, P, F, G> FnTrait1<MsgAsyncID<OneArg>, A, P, (), G> for F
where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> G,
    G: Future<Output = MsgFlow<A>>,
{
}

impl<'a, 'b, F, P, A, G, S> Callable<'a, 'b, MsgAsyncID<OneArg>, F, P, (), G, Msg<'a, A, P>> for S
where
    A: Actor,
    F: FnTrait1<MsgAsyncID<OneArg>, A, P, (), G>,
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

impl<'a, A: Actor, P, F, R> FnTrait1<ReqID<OneArg>, A, P, R, ()> for F where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> ReqFlow<A, R>
{
}

impl<'a, 'b, F, P, A, R, S> Callable<'a, 'b, ReqID<OneArg>, F, P, R, (), Req<'a, A, P, R>> for S
where
    A: Actor,
    F: FnTrait1<ReqID<OneArg>, A, P, R, ()>,
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

impl<'a, A: Actor, P, F, G, R> FnTrait1<ReqAsyncID<OneArg>, A, P, R, G> for F
where
    F: Fn(&'a mut A, &'a mut <A as Actor>::State, P) -> G,
    G: Future<Output = ReqFlow<A, R>>,
{
}

impl<'a, 'b, F, P, A, G, R, S> Callable<'a, 'b, ReqAsyncID<OneArg>, F, P, R, G, Req<'a, A, P, R>>
    for S
where
    A: Actor,
    F: FnTrait1<ReqAsyncID<OneArg>, A, P, R, G>,
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

impl<'a, A: Actor, P, F> FnTrait1<MsgIDNoState<OneArg>, A, P, (), ()> for F where
    F: Fn(&'a mut A, P) -> MsgFlow<A>
{
}

impl<'a, 'b, F, P, A, S> Callable<'a, 'b, MsgIDNoState<OneArg>, F, P, (), (), Msg<'a, A, P>> for S
where
    A: Actor,
    F: FnTrait1<MsgIDNoState<OneArg>, A, P, (), ()>,
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

impl<'a, A: Actor, P, F, G> FnTrait1<MsgAsyncIDNoState<OneArg>, A, P, (), G> for F
where
    F: Fn(&'a mut A, P) -> G,
    G: Future<Output = MsgFlow<A>>,
{
}

impl<'a, 'b, F, P, A, G, S> Callable<'a, 'b, MsgAsyncIDNoState<OneArg>, F, P, (), G, Msg<'a, A, P>>
    for S
where
    A: Actor,
    F: FnTrait1<MsgAsyncIDNoState<OneArg>, A, P, (), G>,
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

impl<'a, A: Actor, P, F, R> FnTrait1<ReqIDNoState<OneArg>, A, P, R, ()> for F where
    F: Fn(&'a mut A, P) -> ReqFlow<A, R>
{
}

impl<'a, 'b, F, P, A, R, S> Callable<'a, 'b, ReqIDNoState<OneArg>, F, P, R, (), Req<'a, A, P, R>>
    for S
where
    A: Actor,
    F: FnTrait1<ReqIDNoState<OneArg>, A, P, R, ()>,
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

impl<'a, A: Actor, P, F, G, R> FnTrait1<ReqAsyncIDNoState<OneArg>, A, P, R, G> for F
where
    F: Fn(&'a mut A, P) -> G,
    G: Future<Output = ReqFlow<A, R>>,
{
}

impl<'a, 'b, F, P, A, G, R, S>
    Callable<'a, 'b, ReqAsyncIDNoState<OneArg>, F, P, R, G, Req<'a, A, P, R>> for S
where
    A: Actor,
    F: FnTrait1<ReqAsyncIDNoState<OneArg>, A, P, R, G>,
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
