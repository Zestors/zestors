use futures::Future;
use std::{intrinsics::transmute, marker::PhantomData, pin::Pin};

use crate::{
    action::{Action, AsyncMsgFn, AsyncReqFn, MsgFn, ReqFn},
    actor::Actor,
    flows::{EventFlow, MsgFlow, ReqFlow},
    inbox::ActionSender,
    messaging::{InternalRequest, Msg, Req},
};

//--------------------------------------------------------------------------------------------------
//  Address definition
//--------------------------------------------------------------------------------------------------

pub struct Address<A: ?Sized + Actor> {
    a: PhantomData<A>,
    sender: ActionSender<A>,
}

impl<'a, 'b, A: Actor> Address<A> {
    pub(crate) fn new(sender: ActionSender<A>) -> Self {
        Self {
            sender,
            a: PhantomData,
        }
    }

    fn new_msg<P>(&self, function: MsgFn<A, P>, params: P) -> Msg<A, P>
    where
        P: Send + 'static,
    {
        let action = Action::new_sync(params, function);
        Msg::new(&self.sender, action)
    }

    fn new_async_msg<F, P>(&self, function: AsyncMsgFn<A, P, F>, params: P) -> Msg<A, P>
    where
        P: Send + 'static,
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        let action = Action::new_async(params, function);
        Msg::new(&self.sender, action)
    }

    fn new_req<P, R>(&self, function: ReqFn<A, P, R>, params: P) -> Req<A, P, R>
    where
        P: Send + 'static,
        R: Send + 'static,
    {
        let (request, reply) = InternalRequest::new();
        let action = Action::new_sync_req(params, request, function);
        Req::new(&self.sender, action, reply)
    }

    fn new_async_req<F, P, R>(&self, function: AsyncReqFn<A, P, F>, params: P) -> Req<A, P, R>
    where
        P: Send + 'static,
        R: Send + 'static,
        F: Future<Output = ReqFlow<A, R>> + Send + 'static,
    {
        let (request, reply) = InternalRequest::new();
        let action = Action::new_async_req(params, request, function);
        Req::new(&self.sender, action, reply)
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

    /// See documentation for [Address::msg]
    fn msg<P>(&self, function: MsgFn<A, P>, params: P) -> Msg<A, P>
    where
        P: Send + 'static,
    {
        self.address().new_msg(function, params)
    }

    /// See documentation for [Address::msg_async]
    fn msg_async<P, F>(&self, function: AsyncMsgFn<A, P, F>, params: P) -> Msg<A, P>
    where
        P: Send + 'static,
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        self.address().new_async_msg(function, params)
    }

    /// See documentation for [Address::req]
    fn req<P, R>(&self, function: ReqFn<A, P, R>, params: P) -> Req<A, P, R>
    where
        R: Send + 'static,
        P: Send + 'static,
    {
        self.address().new_req(function, params)
    }

    /// See documentation for [Address::req_async]
    fn req_async<P, R, F>(&self, function: AsyncReqFn<A, P, F>, params: P) -> Req<A, P, R>
    where
        P: Send + 'static,
        R: Send + 'static,
        F: Future<Output = ReqFlow<A, R>> + Send + 'static,
    {
        self.address().new_async_req(function, params)
    }

    /// Whether this process is still alive.
    fn is_alive(&self) -> bool {
        self.address().sender.is_alive()
    }
}

//--------------------------------------------------------------------------------------------------
//  Call
//--------------------------------------------------------------------------------------------------

pub struct AsyncMsg;
pub struct SyncMsg;
pub struct AsyncReq;
pub struct SyncReq;

pub trait Call<'a, 'b, A: Actor, P, F: 'b, R: 'a, S> {
    fn call(&'a self, function: fn(&'b mut A, &'b mut A::State, P) -> F, params: P) -> R;
}

impl<'a, 'b, A: Actor, P, T> Call<'a, 'b, A, P, MsgFlow<A>, Msg<'a, A, P>, SyncMsg> for T
where
    T: Addressable<A>,
    P: Send + 'static,
{
    fn call(&'a self, function: MsgFn<A, P>, params: P) -> Msg<'a, A, P> {
        self.msg(function, params)
    }
}

impl<'a, 'b, A: Actor, P, F, T> Call<'a, 'b, A, P, F, Msg<'a, A, P>, AsyncMsg> for T
where
    T: Addressable<A>,
    P: 'static + Send,
    F: Future<Output = MsgFlow<A>> + 'static + Send,
{
    fn call(&'a self, function: AsyncMsgFn<A, P, F>, params: P) -> Msg<'a, A, P> {
        self.msg_async(function, params)
    }
}

impl<'a, 'b, A: Actor, P, R, T> Call<'a, 'b, A, P, ReqFlow<A, R>, Req<'a, A, P, R>, SyncReq> for T
where
    T: Addressable<A>,
    P: 'static + Send,
    R: 'static + Send,
{
    fn call(&'a self, function: ReqFn<A, P, R>, params: P) -> Req<'a, A, P, R> {
        self.req(function, params)
    }
}

impl<'a, 'b, A: Actor, P, F, R, T> Call<'a, 'b, A, P, F, Req<'a, A, P, R>, AsyncReq> for T
where
    T: Addressable<A>,
    P: 'static + Send,
    R: 'static + Send,
    F: Future<Output = ReqFlow<A, R>> + 'static + Send,
{
    fn call(&'a self, function: AsyncReqFn<A, P, F>, params: P) -> Req<'a, A, P, R> {
        self.req_async(function, params)
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
        }
    }
}

impl<A: Actor> std::fmt::Debug for Address<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Address")
            .field("sender", &"PacketSender")
            .finish()
    }
}

//--------------------------------------------------------------------------------------------------
//  test
//--------------------------------------------------------------------------------------------------

mod test {
    use super::*;
    use crate::actor::ExitReason;
    use crate::flows::{InitFlow};
    use crate::state::State;

    // fn add_numbers(address: &Address<MyActor>, num1: u32, num2: u32) {
    //     let req = req!(address, |_, _, num1, num2| {
    //         ReqFlow::Reply(num1 + num2)
    //     });
    // }

    #[tokio::test]
    async fn all_functions_dont_segfault_test() -> anyhow::Result<()> {
        let (child, address) = crate::actor::spawn::<MyActor>(MyActor);

        let res1 = address.new_msg(MyActor::test_a, 10).send()?;

        Address::new_msg(&&&&&address, |_, _, _| MsgFlow::Ok, 10)
            .send()
            .unwrap();

        let _val: u32 = 10;

        child.msg(|_, _, _| MsgFlow::Ok, 10).send().unwrap();

        // let res = msg!(address, |_a, _b, val| {
        //     Flow::Ok
        // });

        // let res = msg!(&address, |a, _| {
        //     Flow::Ok
        // });

        // let res = msg!(&address, |_, b| {
        //     Flow::Ok
        // });

        // let res = msg!(&address, |a, b| {
        //     Flow::Ok
        // });

        // let res = req!(&address, |actor, state, val| async {
        //     ReqFlow::Reply(10)
        // });

        let res2 = address.msg_async(MyActor::test_b, 10).send()?;

        let res3 = address
            .req(MyActor::test_c, 10)
            .send()?
            .async_recv()
            .await?;
        let res4 = address
            .req_async(MyActor::test_d, 10)
            .send()?
            .async_recv()
            .await?;

        // let res5 = address.new_msg_nostate(MyActor::test_e, 10).send()?;
        // let res6 = address.new_msg_async_nostate(MyActor::test_f, 10).send()?;

        // let res7 = address
        //     .new_req_nostate(MyActor::test_g, 10)
        //     .send()?
        //     .async_recv()
        //     .await?;
        // let res8 = address
        //     .new_req_async_nostate(MyActor::test_h, 10)
        //     .send()?
        //     .async_recv()
        //     .await?;

        assert_eq!(res1, ());
        assert_eq!(res2, ());
        // assert_eq!(res5, ());
        // assert_eq!(res6, ());

        assert_eq!(res3, "ok");
        assert_eq!(res4, "ok");
        // assert_eq!(res7, "ok");
        // assert_eq!(res8, "ok");

        // let fun = |_, _, _: u32| {
        //     Flow::Ok
        // };

        // let res = child.send(fun!(|_, _, _: u32| {
        //     Flow::Ok
        // }), 10)?;

        // let res1 = address.call(MyActor::test_a, 10).send()?;
        // let res1 = address.call(|actor, state, amount| {
        //     // actor.
        //     Flow::Ok
        // }, 10).send()?;
        // // let res1 = child.send(func!(MyActor::test_a2), ("hi", 10)).await?;

        // let res2 = address.call(MyActor::test_b, 10).send()?;
        // let res3 = address.call(MyActor::test_c, 10).send()?.await?;
        // let res4 = address.call(MyActor::test_d, 10).send()?.await?;

        assert_eq!(res1, ());
        assert_eq!(res2, ());
        // assert_eq!(res5, ());
        // assert_eq!(res6, ());

        assert_eq!(res3, "ok");
        assert_eq!(res4, "ok");
        // assert_eq!(res7, "ok");
        // assert_eq!(res8, "ok");

        Ok(())
    }

    #[derive(Debug)]
    pub struct MyActor;

    #[allow(dead_code, unused_variables)]
    impl MyActor {
        fn test_e(&mut self, c: u32) -> MsgFlow<Self> {
            MsgFlow::Ok
        }

        fn test_a(&mut self, state: &mut State<Self>, c: u32) -> MsgFlow<Self> {
            MsgFlow::Ok
        }

        fn test_a2(&mut self, s: &str, c: u32) -> MsgFlow<Self> {
            MsgFlow::Ok
        }

        async fn test_f<'r>(&'r mut self, c: u32) -> MsgFlow<Self> {
            MsgFlow::Ok
        }

        async fn test_b(&mut self, state: &mut State<Self>, c: u32) -> MsgFlow<Self> {
            MsgFlow::Ok
        }

        fn test_g(&mut self, c: u32) -> ReqFlow<Self, &'static str> {
            ReqFlow::Reply("ok")
        }

        fn test_c(&mut self, state: &mut State<Self>, c: u32) -> ReqFlow<Self, &'static str> {
            ReqFlow::Reply("ok")
        }

        async fn test_h(&mut self, c: u32) -> ReqFlow<Self, &'static str> {
            ReqFlow::Reply("ok")
        }

        async fn test_d(
            &mut self,
            state: &mut <Self as Actor>::State,
            c: u32,
        ) -> ReqFlow<Self, &'static str> {
            ReqFlow::Reply("ok")
        }
    }

    #[allow(dead_code, unused_variables)]
    impl Actor for MyActor {
        type Init = MyActor;

        type ExitWith = u32;

        fn handle_exit(self, state: Self::State, exit: ExitReason<Self>) -> u32 {
            0
        }

        fn init(init: Self::Init, state: &mut Self::State) -> InitFlow<Self> {
            InitFlow::Init(init)
        }
    }
}
