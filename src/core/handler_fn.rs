use crate::core::*;
use futures::{channel::mpsc::SendError, Future};
use std::{
    any::{Any, TypeId},
    error::Error,
    marker::PhantomData,
    mem::transmute,
    ops::Deref,
    pin::Pin,
};

//------------------------------------------------------------------------------------------------
//  Fn!
//------------------------------------------------------------------------------------------------

/// A macro for easily converting handler-`fn`s into `HandleFn`s.
///
/// The `HandleFn` can subsequently be used to send/create `Action`s.
///
/// ## Usage:
/// ```ignore
/// fn handle_message(&mut self, msg: u32, state: ActorState<Self>) -> FlowResult<Self> {
///     Ok(Flow::Cont)
/// }
///
/// let handle_fn = Fn!(handle_message); // is of type "HandlerFn<ActorName, u32, ()>"
/// let action = Action::new(handle_fn, 10); // is of type "Action<ActorName>"
/// ```
#[macro_export]
macro_rules! Fn {
    ($function:expr) => {
        unsafe {
            <$crate::core::HandlerFn<_, _, _> as $crate::core::FromFn<_, _>>::from_fn(
                $function as usize,
                $function,
            )
        }
    };
}

//------------------------------------------------------------------------------------------------
//  HandleFn
//------------------------------------------------------------------------------------------------

/// A handle function is a function that can handle an incoming message for an actor.
///
/// A message is represented as `HandleFn<A, M, ()>`, where `A` is the `Actor` and `M` is the
/// message can be handled.
///
/// A request is represented as `HandleFn<A, M, Request<R>>`, where `R` is the reply that will
/// be sent back when this request is handled.
///
/// Handle functions are used to create actions which can be handled by an actor.
pub struct HandlerFn<A: ?Sized, M, R: RcvPart> {
    phantom_data: (PhantomData<A>, PhantomData<(M, R)>),
    any: UntypedHandlerFn,
}

//------------------------------------------------------------------------------------------------
//  HandlerFn<A, M, R>
//------------------------------------------------------------------------------------------------

impl<A, M, R: RcvPart> HandlerFn<A, M, R> {
    pub(crate) fn into_any(self) -> UntypedHandlerFn {
        self.any
    }
}

//------------------------------------------------------------------------------------------------
//  HandlerFn<A, Snd<M>, ()>
//------------------------------------------------------------------------------------------------

impl<A, M> HandlerFn<A, Snd<M>, ()>
where
    A: Actor,
    M: 'static + Send,
{
    /// Call this function.
    pub async fn call(
        self,
        actor: &mut A,
        state: &mut State<A>,
        params: M,
    ) -> Result<Flow, A::Error> {
        let params = Box::new((params, ()));
        unsafe {
            self.any
                .wrapper
                .call(actor, state, params, self.any.handler_fn)
                .await
        }
    }

    /// Create a new message.
    ///
    /// Instead of using this, prefer to use the `Fn!` macro, which works for all
    /// `HandleFn` types.
    pub fn new_msg_1(handler: MsgHandlerFn1<A, M>) -> Self {
        fn wrapper<A: Actor, M: 'static>(
            actor: &mut A,
            _state: &mut State<A>,
            params: Box<dyn Any + Send>,
            handler: usize,
        ) -> HandlerOutput<A> {
            let (msg, _req): (M, ()) = *params.downcast().unwrap();
            let handler: MsgHandlerFn1<A, M> = unsafe { transmute(handler) };
            handler(actor, msg)
        }

        Self {
            phantom_data: (PhantomData, PhantomData),
            any: UntypedHandlerFn {
                handler_fn: handler as usize,
                wrapper: HandlerWrapper::new_sync(wrapper::<A, M>),
            },
        }
    }

    /// Create a new message.
    ///
    /// Instead of using this, prefer to use the `Fn!` macro, which works for all
    /// `HandleFn` types.
    pub fn new_msg_2(handler: MsgHandlerFn2<A, M>) -> Self {
        fn wrapper<A: Actor, M: 'static>(
            actor: &mut A,
            state: &mut State<A>,
            params: Box<dyn Any + Send>,
            handler: usize,
        ) -> HandlerOutput<A> {
            let (msg, req): (M, ()) = *params.downcast().unwrap();
            let handler: MsgHandlerFn2<A, M> = unsafe { transmute(handler) };
            handler(actor, msg, state)
        }

        Self {
            phantom_data: (PhantomData, PhantomData),
            any: UntypedHandlerFn {
                handler_fn: handler as usize,
                wrapper: HandlerWrapper::new_sync(wrapper::<A, M>),
            },
        }
    }

    /// Create a new message.
    ///
    /// Instead of using this, prefer to use the `Fn!` macro, which works for all
    /// `HandleFn` types.
    pub fn new_async_msg_1<'a, F: HandlerFut<'a, A>>(handler: AsyncMsgHandlerFn1<A, M, F>) -> Self {
        fn wrapper<'a, A: Actor, M: Send + 'static, F: HandlerFut<'a, A>>(
            actor: &'a mut A,
            _state: &'a mut State<A>,
            params: Box<dyn Any + Send>,
            handler: usize,
        ) -> Pin<Box<dyn WrapperFut<'a, A> + 'a>> {
            Box::pin(async move {
                let (msg, _req): (M, ()) = *params.downcast().unwrap();
                let handler: AsyncMsgHandlerFn1<A, M, F> = unsafe { transmute(handler) };
                handler(actor, msg).await
            })
        }

        Self {
            phantom_data: (PhantomData, PhantomData),
            any: UntypedHandlerFn {
                handler_fn: handler as usize,
                wrapper: HandlerWrapper::new_async(wrapper::<A, M, F>),
            },
        }
    }

    /// Create a new message.
    ///
    /// Instead of using this, prefer to use the `Fn!` macro, which works for all
    /// `HandleFn` types.
    pub fn new_async_msg_2<'a, F: HandlerFut<'a, A>>(handler: AsyncMsgHandlerFn2<A, M, F>) -> Self {
        fn wrapper<'a, A: Actor, M: Send + 'static, F: HandlerFut<'a, A>>(
            actor: &'a mut A,
            state: &'a mut State<A>,
            params: Box<dyn Any + Send>,
            handler: usize,
        ) -> Pin<Box<dyn WrapperFut<'a, A> + 'a>> {
            Box::pin(async move {
                let (msg, _req): (M, ()) = *params.downcast().unwrap();
                let handler: AsyncMsgHandlerFn2<A, M, F> = unsafe { transmute(handler) };
                handler(actor, msg, state).await
            })
        }

        Self {
            phantom_data: (PhantomData, PhantomData),
            any: UntypedHandlerFn {
                handler_fn: handler as usize,
                wrapper: HandlerWrapper::new_async(wrapper::<A, M, F>),
            },
        }
    }
}

//------------------------------------------------------------------------------------------------
//  HandlerFn<A, Snd<M>, Rcv<R>>
//------------------------------------------------------------------------------------------------

impl<A, M, R> HandlerFn<A, Snd<M>, Rcv<R>>
where
    A: Actor,
    M: Send + 'static,
    R: Send + 'static,
{
    /// Call this function.
    pub async fn call(
        self,
        actor: &mut A,
        state: &mut State<A>,
        params: M,
        request: Snd<R>,
    ) -> FlowResult<A> {
        let params = Box::new((params, request));
        unsafe {
            self.any
                .wrapper
                .call(actor, state, params, self.any.handler_fn)
                .await
        }
    }

    /// Create a new request.
    ///
    /// Instead of using this, prefer to use the `Fn!` macro, which works for all
    /// `HandleFn` types.
    pub fn new_req_1(handler: ReqHandlerFn1<A, M, R>) -> Self {
        fn wrapper<A: Actor, M: Send + 'static, R: Send + 'static>(
            actor: &mut A,
            _state: &mut State<A>,
            params: Box<dyn Any + Send>,
            handler: usize,
        ) -> HandlerOutput<A> {
            let (msg, req): (M, Snd<R>) = *params.downcast().unwrap();
            let handler: ReqHandlerFn1<A, M, R> = unsafe { transmute(handler) };
            handler(actor, msg, req)
        }

        Self {
            phantom_data: (PhantomData, PhantomData),
            any: UntypedHandlerFn {
                handler_fn: handler as usize,
                wrapper: HandlerWrapper::new_sync(wrapper::<A, M, R>),
            },
        }
    }

    /// Create a new request.
    ///
    /// Instead of using this, prefer to use the `Fn!` macro, which works for all
    /// `HandleFn` types.
    pub fn new_req_2(handler: ReqHandlerFn2<A, M, R>) -> Self {
        fn wrapper<A: Actor, M: Send + 'static, R: Send + 'static>(
            actor: &mut A,
            state: &mut State<A>,
            params: Box<dyn Any + Send>,
            handler: usize,
        ) -> HandlerOutput<A> {
            let (msg, req): (M, Snd<R>) = *params.downcast().unwrap();
            let handler: ReqHandlerFn2<A, M, R> = unsafe { transmute(handler) };
            handler(actor, msg, req, state)
        }

        Self {
            phantom_data: (PhantomData, PhantomData),
            any: UntypedHandlerFn {
                handler_fn: handler as usize,
                wrapper: HandlerWrapper::new_sync(wrapper::<A, M, R>),
            },
        }
    }

    /// Create a new request.
    ///
    /// Instead of using this, prefer to use the `Fn!` macro, which works for all
    /// `HandleFn` types.
    pub fn new_async_req_1<'a, F: HandlerFut<'a, A>>(
        handler: AsyncReqHandlerFn1<A, M, R, F>,
    ) -> Self {
        fn wrapper<'a, A: Actor, M: Send + 'static, R: Send + 'static, F: HandlerFut<'a, A>>(
            actor: &'a mut A,
            _state: &'a mut State<A>,
            params: Box<dyn Any + Send>,
            handler: usize,
        ) -> Pin<Box<dyn WrapperFut<'a, A> + 'a>> {
            Box::pin(async move {
                let (msg, req): (M, Snd<R>) = *params.downcast().unwrap();
                let handler: AsyncReqHandlerFn1<A, M, R, F> = unsafe { transmute(handler) };
                handler(actor, msg, req).await
            })
        }

        Self {
            phantom_data: (PhantomData, PhantomData),
            any: UntypedHandlerFn {
                handler_fn: handler as usize,
                wrapper: HandlerWrapper::new_async(wrapper::<A, M, R, F>),
            },
        }
    }

    /// Create a new request.
    ///
    /// Instead of using this, prefer to use the `Fn!` macro, which works for all
    /// `HandleFn` types.
    pub fn new_async_req_2<'a, F: HandlerFut<'a, A>>(
        handler: AsyncReqHandlerFn2<A, M, R, F>,
    ) -> Self {
        fn wrapper<'a, A: Actor, M: Send + 'static, R: Send + 'static, F: HandlerFut<'a, A>>(
            actor: &'a mut A,
            state: &'a mut State<A>,
            params: Box<dyn Any + Send>,
            handler: usize,
        ) -> Pin<Box<dyn WrapperFut<'a, A> + 'a>> {
            Box::pin(async move {
                let (msg, req): (M, Snd<R>) = *params.downcast().unwrap();
                let handler: AsyncReqHandlerFn2<A, M, R, F> = unsafe { transmute(handler) };
                handler(actor, msg, req, state).await
            })
        }

        Self {
            phantom_data: (PhantomData, PhantomData),
            any: UntypedHandlerFn {
                handler_fn: handler as usize,
                wrapper: HandlerWrapper::new_async(wrapper::<A, M, R, F>),
            },
        }
    }
}

//------------------------------------------------------------------------------------------------
//  UntypedHandlerFn
//------------------------------------------------------------------------------------------------

/// An untyped `HandleFn`, that can be unsafely converted back or called.
#[derive(Debug)]
pub(crate) struct UntypedHandlerFn {
    handler_fn: usize,
    wrapper: HandlerWrapper,
}

impl UntypedHandlerFn {
    /// Call this function.
    pub(crate) async unsafe fn call<A: Actor>(
        self,
        actor: &mut A,
        state: &mut State<A>,
        params: Box<dyn Send + Any>,
    ) -> FlowResult<A> {
        unsafe {
            self.wrapper
                .call(actor, state, params, self.handler_fn)
                .await
        }
    }
}

//------------------------------------------------------------------------------------------------
//  Wrapper
//------------------------------------------------------------------------------------------------

/// This is the wrapper function that can be called by the actor to handle an action.
/// It is a part of the `HandleFn`.
#[derive(Debug)]
enum HandlerWrapper {
    Sync(usize),
    Async(usize),
}

impl HandlerWrapper {
    fn new_sync<A: Actor>(wrapper: SyncWrapper<A>) -> Self {
        Self::Sync(wrapper as usize)
    }

    fn new_async<'a, A: Actor>(wrapper: AsyncWrapper<'a, A>) -> Self {
        Self::Async(wrapper as usize)
    }

    /// Call this function
    async unsafe fn call<A: Actor>(
        self,
        actor: &mut A,
        state: &mut State<A>,
        params: Box<dyn Any + Send>,
        handler: usize,
    ) -> HandlerOutput<A> {
        match self {
            HandlerWrapper::Sync(wrapper) => {
                let wrapper: SyncWrapper<A> = unsafe { transmute(wrapper) };
                wrapper(actor, state, params, handler)
            }
            HandlerWrapper::Async(wrapper) => {
                let wrapper: AsyncWrapper<A> = unsafe { transmute(wrapper) };
                wrapper(actor, state, params, handler).await
            }
        }
    }
}

// The types of the `WrapperFn`s.
type SyncWrapper<A> = fn(&mut A, &mut State<A>, Box<dyn Any + Send>, usize) -> HandlerOutput<A>;
type AsyncWrapper<'a, A> = fn(
    &'a mut A,
    &'a mut State<A>,
    Box<dyn Any + Send>,
    usize,
) -> Pin<Box<dyn WrapperFut<'a, A> + 'a>>;

/// The output of an async `WrapperFn`
trait WrapperFut<'a, A: Actor>: Future<Output = HandlerOutput<A>> + Send + 'a {}
impl<'a, F, A: Actor> WrapperFut<'a, A> for F where F: Future<Output = HandlerOutput<A>> + Send + 'a {}

//------------------------------------------------------------------------------------------------
//  FromFn
//------------------------------------------------------------------------------------------------
/// A trait to help with the conversion of fn pointers to `HandleFn`s.
///
/// This is only necessary because `fn`-types cannot be coerced at all if there are multiple
/// possible implementations. That's why this function is called by a macro, that ensures the
/// ptr is the correct type. Then coercion can be dont using `Fn`-traits, and making sure the
/// `fn`s implement these traits.
pub trait FromFn<F, Ident>: Sized {
    unsafe fn from_fn(ptr: usize, function: F) -> Self;
}

impl<A, M, F> FromFn<F, HandlerIdent1> for HandlerFn<A, Snd<M>, ()>
where
    A: Actor,
    M: Send + 'static,
    F: Fn(&mut A, M) -> FlowResult<A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: MsgHandlerFn1<A, M> = transmute(ptr);
        Self::new_msg_1(ptr)
    }
}

impl<'a, A, M, F> FromFn<F, HandlerIdent2> for HandlerFn<A, Snd<M>, ()>
where
    A: Actor,
    M: Send + 'static,
    F: Fn(&'a mut A, M, &'a mut State<A>) -> FlowResult<A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: MsgHandlerFn2<A, M> = transmute(ptr);
        Self::new_msg_2(ptr)
    }
}

impl<'a, A, M, F, Fut> FromFn<F, AsyncHandlerIdent1<Fut>> for HandlerFn<A, Snd<M>, ()>
where
    A: Actor,
    M: Send + 'static,
    F: Fn(&'a mut A, M) -> Fut,
    Fut: HandlerFut<'a, A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: AsyncMsgHandlerFn1<A, M, Fut> = transmute(ptr);
        Self::new_async_msg_1(ptr)
    }
}

impl<'a, A, M, F, Fut> FromFn<F, AsyncHandlerIdent2<Fut>> for HandlerFn<A, Snd<M>, ()>
where
    A: Actor,
    M: Send + 'static,
    F: Fn(&'a mut A, M, &'a mut State<A>) -> Fut,
    Fut: HandlerFut<'a, A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: AsyncMsgHandlerFn2<A, M, Fut> = transmute(ptr);
        Self::new_async_msg_2(ptr)
    }
}

impl<'a, A, M, F, R> FromFn<F, HandlerIdent1> for HandlerFn<A, Snd<M>, Rcv<R>>
where
    A: Actor,
    R: Send + 'static,
    M: Send + 'static,
    F: Fn(&'a mut A, M, Snd<R>) -> FlowResult<A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: ReqHandlerFn1<A, M, R> = transmute(ptr);
        Self::new_req_1(ptr)
    }
}

impl<'a, A, M, F, R> FromFn<F, HandlerIdent2> for HandlerFn<A, Snd<M>, Rcv<R>>
where
    A: Actor,
    M: Send + 'static,
    R: Send + 'static,
    F: Fn(&'a mut A, M, Snd<R>, &'a mut State<A>) -> FlowResult<A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: ReqHandlerFn2<A, M, R> = transmute(ptr);
        Self::new_req_2(ptr)
    }
}

impl<'a, A, M, F, Fut, R> FromFn<F, AsyncHandlerIdent1<Fut>> for HandlerFn<A, Snd<M>, Rcv<R>>
where
    A: Actor,
    R: Send + 'static,
    M: Send + 'static,
    F: Fn(&'a mut A, M, Snd<R>) -> Fut,
    Fut: HandlerFut<'a, A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: AsyncReqHandlerFn1<A, M, R, Fut> = transmute(ptr);
        Self::new_async_req_1(ptr)
    }
}

impl<'a, A, M, F, Fut, R> FromFn<F, AsyncHandlerIdent2<Fut>> for HandlerFn<A, Snd<M>, Rcv<R>>
where
    A: Actor,
    R: Send + 'static,
    M: Send + 'static,
    F: Fn(&'a mut A, M, Snd<R>, &'a mut State<A>) -> Fut,
    Fut: HandlerFut<'a, A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: AsyncReqHandlerFn2<A, M, R, Fut> = transmute(ptr);
        Self::new_async_req_2(ptr)
    }
}

// Identifiers for having the `FromFn` trait auto-coerce properly.
pub struct HandlerIdent1;
pub struct HandlerIdent2;
pub struct AsyncHandlerIdent1<Fut>(PhantomData<Fut>);
pub struct AsyncHandlerIdent2<Fut>(PhantomData<Fut>);

//-------------------------------------------------
//  Helper types
//-------------------------------------------------

type MsgHandlerFn1<A, M> = fn(&mut A, M) -> FlowResult<A>;
type MsgHandlerFn2<A, M> = fn(&mut A, M, &mut State<A>) -> FlowResult<A>;
type AsyncMsgHandlerFn1<A, M, F> = fn(&mut A, M) -> F;
type AsyncMsgHandlerFn2<A, M, F> = fn(&mut A, M, &mut State<A>) -> F;

type ReqHandlerFn1<A, M, R> = fn(&mut A, M, Snd<R>) -> FlowResult<A>;
type ReqHandlerFn2<A, M, R> = fn(&mut A, M, Snd<R>, &mut State<A>) -> FlowResult<A>;
type AsyncReqHandlerFn1<A, M, R, F> = fn(&mut A, M, Snd<R>) -> F;
type AsyncReqHandlerFn2<A, M, R, F> = fn(&mut A, M, Snd<R>, &mut State<A>) -> F;

pub trait HandlerFut<'a, A: Actor>: Future<Output = FlowResult<A>> + Send + 'a {}
impl<'a, A: Actor, F> HandlerFut<'a, A> for F where F: Future<Output = FlowResult<A>> + Send + 'a {}
pub(crate) type HandlerOutput<A> = FlowResult<A>;

//------------------------------------------------------------------------------------------------
//  Testing
//------------------------------------------------------------------------------------------------

pub mod test {
    use super::*;
    use crate as zestors;
    use zestors_codegen::{Addr, NoScheduler, Actor};

    #[derive(Addr, NoScheduler, Actor)]
    pub struct MyActor;

    impl MyActor {
        fn msg1(&mut self, msg: u32) -> FlowResult<Self> {
            todo!()
        }

        fn msg2(&mut self, msg: u32, state: &mut State<Self>) -> FlowResult<Self> {
            todo!()
        }

        pub async fn async_msg1(&mut self, msg: u32) -> FlowResult<Self> {
            todo!()
        }

        async fn async_msg2(&mut self, msg: u32, state: &mut State<Self>) -> FlowResult<Self> {
            todo!()
        }

        fn req1(&mut self, msg: u32, request: Snd<u64>) -> FlowResult<Self> {
            todo!()
        }

        fn req2(&mut self, msg: u32, req: Snd<u64>, state: &mut State<Self>) -> FlowResult<Self> {
            todo!()
        }

        async fn async_req1(&mut self, msg: u32, req: Snd<u64>) -> FlowResult<Self> {
            Ok(Flow::Cont)
        }

        async fn async_req2(
            &mut self,
            msg: u32,
            req: Snd<u64>,
            state: &mut State<Self>,
        ) -> FlowResult<Self> {
            Ok(Flow::Cont)
        }
    }

    fn all_function_types_compile_test(addr: MyActorAddr) {
        let () = addr.call(Fn!(MyActor::msg1), 10).unwrap();
        let () = addr.call(Fn!(MyActor::msg2), 10).unwrap();
        let () = addr.call(Fn!(MyActor::async_msg1), 10).unwrap();
        let () = addr.call(Fn!(MyActor::async_msg2), 10).unwrap();
        let _: Rcv<u64> = addr.call(Fn!(MyActor::req1), 10).unwrap();
        let _: Rcv<u64> = addr.call(Fn!(MyActor::req2), 10).unwrap();
        let _: Rcv<u64> = addr.call(Fn!(MyActor::async_req1), 10).unwrap();
        let _: Rcv<u64> = addr.call(Fn!(MyActor::async_req2), 10).unwrap();

        Fn!(MyActor::msg1);
        Fn!(MyActor::msg2);
        Fn!(MyActor::async_msg1);
        Fn!(MyActor::async_msg2);
        Fn!(MyActor::req1);
        Fn!(MyActor::req2);
        Fn!(MyActor::async_req1);
        Fn!(MyActor::async_req2);
    }
}
