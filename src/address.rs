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
    errors::DidntArrive,
    flows::{EventFlow, MsgFlow, ReqFlow},
    function::{MsgFn, ReqFn},
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

impl<A: Actor> Address<A> {
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

    pub fn msg<'a, P>(&'a self, function: MsgFn<A, P>, params: P) -> Msg<'a, A, P>
    where
        P: Send + 'static,
    {
        Msg::new(&self.sender, Action::new(function, params))
    }

    pub fn req<P, R>(&self, function: ReqFn<A, P, R>, params: P) -> Req<A, P, R>
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
    fn address(&self) -> &Address<A>;

    /// Whether this process is still alive.
    fn is_alive(&self) -> bool {
        self.address().sender.is_alive()
    }

    fn process_id(&self) -> ProcessId {
        self.address().process_id()
    }
}

//--------------------------------------------------------------------------------------------------
//  Implement traits for Address
//--------------------------------------------------------------------------------------------------

impl<A: Actor> Addressable<A> for Address<A> {
    fn address(&self) -> &Address<A> {
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

pub trait Callable<'a, A, F, P, R, X: 'a, Y> {
    fn call(&'a self, function: F, params: P) -> X;
    fn send(&'a self, function: F, params: P) -> Y;
}

impl<'a, A: Actor, P, T>
    Callable<'a, A, MsgFn<A, P>, P, (), Msg<'a, A, P>, Result<(), DidntArrive<P>>> for T
where
    P: Send + 'static,
    T: Addressable<A>,
{
    fn call(&'a self, function: MsgFn<A, P>, params: P) -> Msg<'a, A, P> {
        self.address().msg(function, params)
    }

    fn send(&'a self, function: MsgFn<A, P>, params: P) -> Result<(), DidntArrive<P>> {
        self.call(function, params).send()
    }
}

impl<'a, A: Actor, P, R, T>
    Callable<'a, A, ReqFn<A, P, R>, P, R, Req<'a, A, P, R>, Result<Reply<R>, DidntArrive<P>>> for T
where
    P: Send + 'static,
    R: Send + 'static,
    T: Addressable<A>,
{
    fn call(&'a self, function: ReqFn<A, P, R>, params: P) -> Req<'a, A, P, R> {
        self.address().req(function, params)
    }

    fn send(&'a self, function: ReqFn<A, P, R>, params: P) -> Result<Reply<R>, DidntArrive<P>> {
        self.call(function, params).send()
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
        context::BasicCtx,
        flows::{InitFlow, MsgFlow, ReqFlow},
        messaging::Request,
        Fn,
    };

    use super::{Address, Callable};

    #[tokio::test]
    pub async fn main() {
        let child = actor::spawn::<MyActor>(());
        let res1 = child.send(Fn!(MyActor::test_a), 10).unwrap();
        let res2 = child.send(Fn!(MyActor::test_b), 10).unwrap();
        let res3 = child.send(Fn!(MyActor::test_c), 10).unwrap().await.unwrap();
        let res4 = child.send(Fn!(MyActor::test_d), 10).unwrap().await.unwrap();
        let res5 = child.send(Fn!(MyActor::test_e), 10).unwrap().await.unwrap();
        let res6 = child.send(Fn!(MyActor::test_f), 10).unwrap().await.unwrap();

        println!(
            "{:?}, {:?}, {:?}, {:?}, {:?}, {:?}",
            res1, res2, res3, res4, res5, res6
        );
    }

    #[derive(Debug)]
    pub struct MyActor {
        ctx: BasicCtx<Self>,
    }

    #[allow(dead_code, unused_variables)]
    impl MyActor {
        fn test_a(&mut self, c: u32) -> MsgFlow<Self> {
            println!("test_a: {}", c);
            MsgFlow::Ok
        }

        async fn test_b(&mut self, c: u32) -> MsgFlow<Self> {
            println!("test_b: {}", c);
            MsgFlow::Ok
        }

        fn test_c(&mut self, c: u32) -> ReqFlow<Self, &'static str> {
            println!("test_c: {}", c);
            ReqFlow::Reply("ok")
        }

        async fn test_d(&mut self, c: u32) -> ReqFlow<Self, &'static str> {
            println!("test_d: {}", c);
            ReqFlow::Reply("ok")
        }

        fn test_e(&mut self, c: u32, request: Request<&'static str>) -> MsgFlow<Self> {
            println!("test_e: {}", c);
            request.reply("ok");
            MsgFlow::Ok
        }

        async fn test_f(&mut self, c: u32, request: Request<&'static str>) -> MsgFlow<Self> {
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
                ctx: BasicCtx::new(address),
            })
        }

        type Ctx = BasicCtx<Self>;

        fn ctx(&mut self) -> &mut Self::Ctx {
            &mut self.ctx
        }
    }
}
