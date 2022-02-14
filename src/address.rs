use futures::Future;
use std::intrinsics::transmute;

use crate::{
    actor::Actor,
    flows::{InternalFlow, MsgFlow, ReqFlow},
    messaging::{Msg, Req},
    packet::{HandlerFn, HandlerPacket, Packet},
    inbox::PacketSender,
};

//--------------------------------------------------------------------------------------------------
//  Address definition
//--------------------------------------------------------------------------------------------------

pub struct Address<A: Actor + ?Sized> {
    sender: PacketSender<A>,
}

impl<'a, 'b, A: Actor> Address<A> {
    pub(crate) fn new(sender: PacketSender<A>) -> Self {
        Self { sender }
    }

    /// Whether this actor is still alive
    pub fn is_alive(&self) -> bool {
        self.sender.is_alive()
    }

    /// Create a new [Msg] with a handler function that can be sent to this actor.
    pub fn msg<P: Send + 'static>(&'a self, params: P, function: MsgFn<A, P>) -> Msg<'a, A, P> {
        let handler_fn = HandlerFn::Sync(
            |actor: &mut A,
             state: &mut <A as Actor>::State,
             handler_packet: HandlerPacket|
             -> InternalFlow<A> {
                // transmute the fn_ptr
                let function: MsgFn<A, P> = unsafe { transmute(handler_packet.fn_ptr()) };
                // downcast the packet data to params and the request
                let params = handler_packet.downcast_msg();
                // call the function with everything necessary
                function(actor, state, params).into_internal()
            },
        );

        // Create the packet from the handler fn and params
        let packet = Packet::new_msg(handler_fn, function as usize, params);

        // Create a request from the packet and the reply
        Msg::new(&self.sender, packet)
    }

    /// Create a new [Msg] with an `async` handler function that can be sent to this actor.
    pub fn msg_async<P: Send + 'static, F>(
        &'a self,
        params: P,
        function: AsyncMsgFn<A, P, F>,
    ) -> Msg<'a, A, P>
    where
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        let handler_fn = HandlerFn::Async(
            |actor: &mut A, state: &mut <A as Actor>::State, handler_packet: HandlerPacket| {
                // transmute the fn_ptr
                let function: AsyncMsgFn<A, P, F> = unsafe { transmute(handler_packet.fn_ptr()) };
                // downcast the packet data to params and the request
                let params = handler_packet.downcast_msg();

                // now we transform the async function and box it
                Box::pin(async move {
                    // call and await the function
                    function(actor, state, params).await.into_internal()
                })
            },
        );

        // Create the packet from the handler fn and params
        let packet = Packet::new_msg(handler_fn, function as usize, params);

        // Create a request from the packet and the reply
        Msg::new(&self.sender, packet)
    }

    /// Create a new [Req] with a handler function that can be sent to this actor.
    pub fn req<P: Send + 'static, R: Send + 'static>(
        &'a self,
        params: P,
        function: ReqFn<A, P, R>,
    ) -> Req<'a, A, P, R> {
        let handler_fn = HandlerFn::Sync(
            |actor: &mut A,
             state: &mut <A as Actor>::State,
             handler_packet: HandlerPacket|
             -> InternalFlow<A> {
                // transmute the fn_ptr
                let function: ReqFn<A, P, R> = unsafe { transmute(handler_packet.fn_ptr()) };
                // downcast the packet data to params and the request
                let (params, request) = handler_packet.downcast_req();
                // call the function with everything necessary
                let (msg_flow, reply) = function(actor, state, params).take_reply();
                // send back a reply
                if let Some(reply) = reply {
                    request.reply(reply);
                }
                // return the msg_flow
                msg_flow
            },
        );

        // Create the packet from the handler fn and params
        let (packet, reply) = Packet::new_req(handler_fn, function as usize, params);

        // Create a request from the packet and the reply
        Req::new(&self.sender, packet, reply)
    }

    /// Create a new [Req] with an `async` handler function that can be sent to this actor.
    pub fn req_async<P: Send + 'static, R: Send + 'static, F>(
        &'a self,
        params: P,
        function: AsyncReqFn<A, P, F>,
    ) -> Req<'a, A, P, R>
    where
        F: Future<Output = ReqFlow<A, R>> + Send + 'static,
    {
        let handler_fn = HandlerFn::Async(
            |actor: &mut A, state: &mut <A as Actor>::State, handler_packet: HandlerPacket| {
                // transmute the fn_ptr
                let function: AsyncReqFn<A, P, F> = unsafe { transmute(handler_packet.fn_ptr()) };
                // downcast the packet data to params and the request
                let (params, request) = handler_packet.downcast_req();

                // now we transform the async function and box it
                Box::pin(async move {
                    // call and await the function
                    let (msg_flow, reply) = function(actor, state, params).await.take_reply();
                    // send back a reply
                    if let Some(reply) = reply {
                        request.reply(reply);
                    }
                    // return the MsgFlow
                    msg_flow
                })
            },
        );

        // Create the packet from the handler fn and params
        let (packet, reply) = Packet::new_req(handler_fn, function as usize, params);

        // Create a request from the packet and the reply
        Req::new(&self.sender, packet, reply)
    }
}

//--------------------------------------------------------------------------------------------------
//  Addressable + RawAddress traits
//--------------------------------------------------------------------------------------------------

/// A trait which must be implemented for every custom address. If you would like to be able to
/// directly call `address.msg()` etc on this custom address, you can implement [Addressable] as
/// well.
/// 
/// This can be derived using [crate::derive::Address].
pub trait RawAddress {
    type Actor: Actor;
    fn raw_address(&self) -> &Address<Self::Actor>;
}

/// A trait that allows a custom address to be callable with `address.msg()`, or `address.req_async()`
/// etc. You will probably also want to implement `From<Address<Actor>>` for this custom address,
/// to allow this address to be used as the associated type [Actor::Address].
///
/// This trait has no methods that need to be implemented, but it is necessary to implement
/// [RawAddress] in order to use this trait.
/// 
/// This can be derived using [crate::derive::Addressable].
pub trait Addressable<A: Actor>: RawAddress<Actor = A> {
    
    /// See documentation for [Address::msg]
    fn msg<P>(&self, params: P, function: MsgFn<A, P>) -> Msg<A, P>
    where
        P: Send + 'static,
    {
        self.raw_address().msg(params, function)
    }

    /// See documentation for [Address::msg_async]
    fn msg_async<P, F>(&self, params: P, function: AsyncMsgFn<A, P, F>) -> Msg<A, P>
    where
        P: Send + 'static,
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        self.raw_address().msg_async(params, function)
    }

    /// See documentation for [Address::req]
    fn req<P, R>(&self, params: P, function: ReqFn<A, P, R>) -> Req<A, P, R>
    where
        R: Send + 'static,
        P: Send + 'static,
    {
        self.raw_address().req(params, function)
    }

    /// See documentation for [Address::req_async]
    fn req_async<P, R, F>(&self, params: P, function: AsyncMsgFn<A, P, F>) -> Req<A, P, R>
    where
        P: Send + 'static,
        R: Send + 'static,
        F: Future<Output = ReqFlow<A, R>> + Send + 'static,
    {
        self.raw_address().req_async(params, function)
    }
}

//--------------------------------------------------------------------------------------------------
//  Implement traits for Address
//--------------------------------------------------------------------------------------------------

impl<A: Actor> RawAddress for Address<A> {
    type Actor = A;

    fn raw_address(&self) -> &Address<A> {
        self
    }
}

impl<A: Actor> Addressable<A> for Address<A> {}

impl<A: Actor> Clone for Address<A> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<A: Actor> std::fmt::Debug for Address<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Address").field("sender", &"PacketSender").finish()
    }
}

//--------------------------------------------------------------------------------------------------
//  helper types
//--------------------------------------------------------------------------------------------------

pub(crate) type MsgFn<'b, A, P> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> MsgFlow<A>;
pub(crate) type AsyncMsgFn<'b, A, P, F> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> F;
pub(crate) type ReqFn<'b, A, P, R> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> ReqFlow<A, R>;
pub(crate) type AsyncReqFn<'b, A, P, F> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> F;

//--------------------------------------------------------------------------------------------------
//  test
//--------------------------------------------------------------------------------------------------

mod test {
    use super::*;
    use crate::actor::ExitReason;
    use crate::flows::{ExitFlow, InitFlow};
    use crate::state::State;

    // fn add_numbers(address: &Address<MyActor>, num1: u32, num2: u32) {
    //     let req = req!(address, |_, _, num1, num2| {
    //         ReqFlow::Reply(num1 + num2)
    //     });
    // }

    #[tokio::test]
    async fn all_functions_dont_segfault_test() -> anyhow::Result<()> {
        let (child, address) = crate::actor::spawn::<MyActor>(MyActor);

        let res1 = address.msg(10, MyActor::test_a).send()?;

        Address::msg(&&&&&address, 10, |_, _, _| MsgFlow::Ok);

        let _val: u32 = 10;

        child.msg(10, |_, _, _| MsgFlow::Ok);

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

        let res2 = address.msg_async(10, MyActor::test_b).send()?;

        let res3 = address
            .req(10, MyActor::test_c)
            .send()?
            .async_recv()
            .await?;
        let res4 = address
            .req_async(10, MyActor::test_d)
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
        type ExitWith = u32;

        fn handle_exit(self, state: &mut Self::State, exit: ExitReason<Self>) -> ExitFlow<Self> {
            ExitFlow::ContinueExit(0)
        }

        fn init(init: Self::Init, state: &mut Self::State) -> InitFlow<Self> {
            InitFlow::Init(init)
        }
    }
}
