use super::*;
use crate::{core::*, Fn};

use futures::{channel::mpsc::SendError, Future};
use std::{
    any::{Any, TypeId},
    marker::PhantomData,
    mem::transmute,
    pin::Pin,
};

//------------------------------------------------------------------------------------------------
//  HandlerFn
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

impl<A, M, R: RcvPart> HandlerFn<A, M, R> {
    pub(crate) fn into_any(self) -> UntypedHandlerFn {
        self.any
    }
}

//------------------------------------------------------------------------------------------------
//  HandlerFn<A, M, ()>
//------------------------------------------------------------------------------------------------

impl<A, M> HandlerFn<A, M, ()>
where
    A: Actor,
    M: 'static + Send,
{
    /// Call this function.
    pub async fn call(self, actor: &mut A, state: &mut State<A>, params: M) -> FlowResult<A> {
        let params = Box::new((params, ()));
        unsafe {
            self.any
                .wrapper
                .call(actor, state, params, self.any.handler_fn)
                .await
        }
    }

    /// Create a new HandlerFn.
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

    /// Create a new HandlerFn.
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

    /// Create a new HandlerFn.
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

    /// Create a new HandlerFn.
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
//  HandlerFn<A, M, Rcv<R>>
//------------------------------------------------------------------------------------------------

impl<A, M, R> HandlerFn<A, M, Rcv<R>>
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

    /// Create a new HandlerFn.
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

    /// Create a new HandlerFn.
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

    /// Create a new HandlerFn.
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

    /// Create a new HandlerFn.
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
        self.wrapper
            .call(actor, state, params, self.handler_fn)
            .await
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
                let wrapper: SyncWrapper<A> = transmute(wrapper);
                wrapper(actor, state, params, handler)
            }
            HandlerWrapper::Async(wrapper) => {
                let wrapper: AsyncWrapper<A> = transmute(wrapper);
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
//  Testing
//------------------------------------------------------------------------------------------------

pub mod test {
    use super::*;
    use crate as zestors;
    use zestors_codegen::{Actor, Addr, NoScheduler};

    #[derive(Addr, NoScheduler, Actor)]
    pub struct MyActor;

    impl MyActor {
        fn msg1(&mut self, msg: u32) -> FlowResult<Self> {
            unimplemented!()
        }
        fn msg2(&mut self, msg: u32, state: &mut State<Self>) -> FlowResult<Self> {
            unimplemented!()
        }
        pub async fn async_msg1(&mut self, msg: u32) -> FlowResult<Self> {
            unimplemented!()
        }
        async fn async_msg2(&mut self, msg: u32, state: &mut State<Self>) -> FlowResult<Self> {
            unimplemented!()
        }
        fn req1(&mut self, msg: u32, request: Snd<u64>) -> FlowResult<Self> {
            unimplemented!()
        }
        fn req2(&mut self, msg: u32, req: Snd<u64>, state: &mut State<Self>) -> FlowResult<Self> {
            unimplemented!()
        }
        async fn async_req1(&mut self, msg: u32, req: Snd<u64>) -> FlowResult<Self> {
            unimplemented!()
        }
        async fn async_req2(
            &mut self,
            msg: u32,
            req: Snd<u64>,
            state: &mut State<Self>,
        ) -> FlowResult<Self> {
            unimplemented!()
        }
    }

    #[allow(unused)]
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
