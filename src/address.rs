use futures::Future;
use std::{
    any::Any,
    fmt::Debug,
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use crate::{
    action::{
        Action, AsyncMsgFnType, AsyncReqFn2Type, AsyncReqFnType, MsgFnType, ReqFn2Type, ReqFnType,
    },
    actor::{Actor, ProcessId},
    distributed::pid::ProcessRef,
    errors::{DidntArrive, ReqRecvError},
    flows::{EventFlow, MsgFlow, ReqFlow},
    function::{MsgFn, RefSafe, ReqFn},
    inbox::ActionSender,
    messaging::{Msg, Reply, Req, Request},
};

//--------------------------------------------------------------------------------------------------
//  Address definition
//--------------------------------------------------------------------------------------------------

pub struct Address<A: ?Sized> {
    a: PhantomData<A>,
    sender: ActionSender<A>,
    process_id: ProcessId,
}

unsafe impl<A> Sync for Address<A> {}

impl<'params, A: Actor> Address<A> {
    pub(crate) fn new(sender: ActionSender<A>, process_id: ProcessId) -> Self {
        Self {
            sender,
            a: PhantomData,
            process_id,
        }
    }

    pub fn process_id(&self) -> ProcessId {
        self.process_id
    }

    pub async fn req_recv_old<P, R>(
        &self,
        function: ReqFn<'params, A, P, R, RefSafe>,
        params: P,
    ) -> Result<R, ReqRecvError<P>>
    where
        P: Send + 'params,
        R: Send + 'params,
    {
        Ok(unsafe { self.req_ref(function, params) }?.await?)
    }

    pub async fn req_recv_sync<'a, P, R>(
        &self,
        function: ReqFnType<'a, A, P, R>,
        params: P,
    ) -> Result<R, ReqRecvError<P>>
    where
        P: Send + 'a,
        R: Send + 'a,
    {
        let (request, reply) = Request::new();
        let request = unsafe {
            Req::new(
                &self.sender,
                Action::new_req_ref::<'a, _, _, RefSafe>(
                    ReqFn::new_sync(function),
                    params,
                    request,
                ),
                reply,
            )
            .send_ref()
        };

        Ok(request?.await?)
    }

    pub fn msg<'a, P>(&'a self, function: MsgFn<A, P>, params: P) -> Result<(), DidntArrive<P>>
    where
        P: Send + 'static,
    {
        Msg::new(&self.sender, Action::new(function, params)).send()
    }

    pub unsafe fn req_ref<'a, P, R>(
        &self,
        function: ReqFn<A, P, R, RefSafe>,
        params: P,
    ) -> Result<Reply<R>, DidntArrive<P>>
    where
        P: Send + 'a,
        R: Send + 'a,
    {
        let (request, reply) = Request::new();
        Req::new(
            &self.sender,
            Action::new_req_ref::<'a, _, _, RefSafe>(function, params, request),
            reply,
        )
        .send_ref()
    }

    pub(crate) fn send_raw<P: Send + 'static>(
        &self,
        action: Action<A>,
    ) -> Result<(), DidntArrive<P>> {
        Msg::new(&self.sender, action).send()
    }

    pub fn req<P, R, S>(
        &self,
        function: ReqFn<A, P, R, S>,
        params: P,
    ) -> Result<Reply<R>, DidntArrive<P>>
    where
        P: Send + 'static,
        R: Send + 'static,
    {
        let (request, reply) = Request::new();
        Req::new(
            &self.sender,
            Action::new_req(function, params, request),
            reply,
        )
        .send()
    }
}

//------------------------------------------------------------------------------------------------
//  AnyAddress
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct AnyAddress(Box<dyn Any + Send + Sync>);

impl AnyAddress {
    pub fn new<A: Actor>(address: Address<A>) -> Self {
        Self(Box::new(address))
    }

    pub fn downcast<A: 'static + Actor>(self) -> Result<Address<A>, Self> {
        match self.0.downcast() {
            Ok(pid) => Ok(*pid),
            Err(boxed) => Err(Self(boxed)),
        }
    }

    pub fn downcast_ref<A: Actor>(&self) -> Option<&Address<A>> {
        self.0.downcast_ref()
    }
}

//--------------------------------------------------------------------------------------------------
//  Addressable + RawAddress traits
//--------------------------------------------------------------------------------------------------

/// A trait that allows a custom address to be callable with `address.msg()`, or `address.req_async()`
/// etc. You will probably also want to implement `From<Address<Actor>>` for this custom address,
/// to allow this address to be used as the associated type [Actor::Address].
///
/// This trait has no methods that need to be implemented, but it is necessary to implement
/// [RawAddress] in order to use this trait.
///
/// This can be derived using [crate::derive::Addressable].
pub trait Addressable<A: Actor> {
    fn addr(&self) -> &Address<A>;

    /// Whether this process is still alive.
    fn is_alive(&self) -> bool {
        self.addr().sender.is_alive()
    }

    fn process_id(&self) -> ProcessId {
        self.addr().process_id()
    }
}

//--------------------------------------------------------------------------------------------------
//  Implement traits for Address
//--------------------------------------------------------------------------------------------------

impl<A: Actor> Addressable<A> for Address<A> {
    fn addr(&self) -> &Address<A> {
        self
    }
}

impl<A: Actor> Clone for Address<A> {
    fn clone(&self) -> Self {
        Self {
            a: PhantomData,
            sender: self.sender.clone(),
            process_id: self.process_id.clone(),
        }
    }
}

impl<A: Actor> std::fmt::Debug for Address<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Address")
            .field("name", &<A as Actor>::debug())
            .field("id", &self.process_id)
            .finish()
    }
}

//------------------------------------------------------------------------------------------------
//  Sendable
//------------------------------------------------------------------------------------------------

#[macro_export]
macro_rules! call {
    ($address:expr, $function:expr, $($param:expr),*) => {{
        use $crate::address::Callable;
        use $crate::Fn;
        $address.call(Fn!($function), ($($param)*))
    }};
}

pub trait Callable<'a, A, F, P, R, X: 'a, S> {
    fn call(&'a self, function: F, params: P) -> X;
}

impl<'a, A: Actor, P, T> Callable<'a, A, MsgFn<A, P>, P, (), Result<(), DidntArrive<P>>, ()> for T
where
    P: Send + 'static,
    T: Addressable<A>,
{
    fn call(&'a self, function: MsgFn<A, P>, params: P) -> Result<(), DidntArrive<P>> {
        self.addr().msg(function, params)
    }
}

impl<'a, A: Actor, P, R, T, S>
    Callable<'a, A, ReqFn<'static, A, P, R, S>, P, R, Result<Reply<R>, DidntArrive<P>>, S> for T
where
    P: Send + 'static,
    R: Send + 'static,
    T: Addressable<A>,
{
    fn call(&'a self, function: ReqFn<A, P, R, S>, params: P) -> Result<Reply<R>, DidntArrive<P>> {
        self.addr().req(function, params)
    }
}

//--------------------------------------------------------------------------------------------------
//  test
//--------------------------------------------------------------------------------------------------

mod test {
    use std::pin::Pin;

    use async_trait::async_trait;
    use futures::Future;

    use crate::{
        actor::{self, Actor, ExitReason},
        address::Addressable,
        context::BasicContext,
        flows::{InitFlow, MsgFlow, ReqFlow},
        function::ReqFn,
        messaging::Request,
        Fn,
    };

    use super::{Address, Callable};

    #[tokio::test]
    pub async fn main() {
        let test = 10;
        let child = actor::spawn::<MyActor>(());

        let res1 = child.call(Fn!(MyActor::test_a), &10).unwrap();
        let res2 = child.call(Fn!(MyActor::test_b), &10).unwrap();

        // let res3 = call!(child, MyActor::test_a, &10).unwrap();
        // let res3 = req!(child, MyActor::test_a, &10).unwrap().await.unwrap();
        // let res3 = msg!(child, MyActor::test_a, &10).unwrap();
        // let res3 = req_recv!(child, MyActor::test_a, &10).await.unwrap();
        // let action1 = action!(MyActor::test_a, &10);
        // let (action2, request) = action_req!(MyActor::test_a, &10);

        let res3 = child
            .addr()
            .req(Fn!(MyActor::test_c), &10)
            .unwrap()
            .await
            .unwrap();
        let res4 = child
            .call(Fn!(MyActor::test_d), &10)
            .unwrap()
            .await
            .unwrap();
        let res5 = child
            .call(Fn!(MyActor::test_e), &10)
            .unwrap()
            .await
            .unwrap();
        let res6 = child
            .call(Fn!(MyActor::test_f), &10)
            .unwrap()
            .await
            .unwrap();

        // let res3 = child
        //     .addr()
        //     .req_recv_ref_safe(Fn!(MyActor::test_c), &test)
        //     .await
        //     .unwrap();
        // let res4 = child
        //     .addr()
        //     .req_recv_ref_safe(Fn!(MyActor::test_d), &test)
        //     .await
        //     .unwrap();
        // let res5 = child
        //     .addr()
        //     .req_recv_old(Fn!(MyActor::test_e), &10)
        //     .await
        //     .unwrap();
        // let res6 = child
        //     .addr()
        //     .req_recv_old(Fn!(MyActor::test_f), &10)
        //     .await
        //     .unwrap();

        println!(
            "{:?}, {:?}, {:?}, {:?}, {:?}, {:?}",
            res1, res2, res3, res4, res5, res6
        );
    }

    #[derive(Debug)]
    pub struct MyActor {
        ctx: BasicContext<Self>,
    }

    #[allow(dead_code, unused_variables)]
    impl MyActor {
        fn test_a(&mut self, c: &u32) -> MsgFlow<Self> {
            println!("test_a: {}", c);
            MsgFlow::Ok
        }

        async fn test_b(&mut self, c: &u32) -> MsgFlow<Self> {
            println!("test_b: {}", c);
            MsgFlow::Ok
        }

        fn test_c(&mut self, c: &u32) -> ReqFlow<Self, &'static str> {
            println!("test_c: {}", c);
            ReqFlow::Reply("ok")
        }

        async fn test_d(&mut self, c: &u32) -> ReqFlow<Self, &'static str> {
            println!("test_d: {}", c);
            ReqFlow::Reply("ok")
        }

        fn test_e(&mut self, c: &u32, request: Request<&'static str>) -> MsgFlow<Self> {
            println!("test_e: {}", c);
            request.reply("ok");
            MsgFlow::Ok
        }

        async fn test_f(&mut self, c: &u32, request: Request<&'static str>) -> MsgFlow<Self> {
            println!("test_f: {}", c);
            request.reply("ok");
            MsgFlow::Ok
        }
    }

    #[allow(dead_code, unused_variables)]
    #[async_trait]
    impl Actor for MyActor {
        type Init = ();

        type ExitWith = u32;

        async fn exit(self, exit: ExitReason<Self>) -> u32 {
            0
        }

        async fn init(init: Self::Init, address: Address<Self>) -> InitFlow<Self> {
            InitFlow::Init(Self {
                ctx: BasicContext::new(address),
            })
        }

        type Context = BasicContext<Self>;

        fn ctx(&mut self) -> &mut Self::Context {
            &mut self.ctx
        }
    }
}
