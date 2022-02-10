use futures::Future;
use std::intrinsics::transmute;
use zestors_codegen::{register_once, static_assert_registered};

use crate::{
    actor::Actor,
    callable::{AsyncMsgFn, AsyncReqFn, MsgFn, ReqFn},
    flows::{Flow, InternalFlow, ReqFlow},
    messaging::{Msg, PacketSender, Req},
    packets::{HandlerFn, HandlerPacket, Packet},
    process::Process,
};

//--------------------------------------------------------------------------------------------------
//  Addressable and FromAddress traits
//--------------------------------------------------------------------------------------------------

/// A trait that allows a custom address to be callable with `address.call()`, or `address.send()`
/// etc. You will probably also want to implement `From<Address<Actor>>` for this custom address,
/// to allow this address to be used as the associated type [Actor::Address].
///
/// todo:
/// A `#[derive(Address)]` macro.
pub trait Addressable<A: Actor> {
    fn raw_address(&self) -> &Address<A>;
}

//--------------------------------------------------------------------------------------------------
//  implement Addressable and FromAddress for Address and Process
//--------------------------------------------------------------------------------------------------

impl<A: Actor> Addressable<A> for Address<A> {
    fn raw_address(&self) -> &Address<A> {
        self
    }
}

impl<A: Actor> Addressable<A> for Process<A> {
    fn raw_address(&self) -> &Address<A> {
        &self.address().raw_address()
    }
}

//--------------------------------------------------------------------------------------------------
//  Address
//--------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Address<A: Actor + ?Sized> {
    sender: PacketSender<A>,
}

//--------------------------------------------------------------------------------------------------
//  Address: public methods
//--------------------------------------------------------------------------------------------------

impl<'a, 'b, A: Actor> Address<A> {
    /// Whether this actor is still alive
    pub fn is_alive(&self) -> bool {
        self.sender.is_alive()
    }
}

//--------------------------------------------------------------------------------------------------
//  Address: private methods
//--------------------------------------------------------------------------------------------------

impl<'a, 'b, A: Actor> Address<A> {
    pub(crate) fn new(sender: PacketSender<A>) -> Self {
        Self { sender }
    }

    pub(crate) fn new_msg<P: Send + 'static>(
        &'a self,
        function: MsgFn<A, P>,
        params: P,
    ) -> Msg<'a, A, P> {
        let handler_fn = HandlerFn::Sync(
            |actor: &mut A,
             state: &mut <A as Actor>::State,
             handler_packet: HandlerPacket|
             -> InternalFlow<A> {
                // transmute the fn_ptr
                let function: MsgFn<A, P> = unsafe { transmute(handler_packet.fn_ptr) };
                // downcast the packet data to params and the request
                let params = handler_packet.data.downcast_msg().unwrap();
                // call the function with everything necessary
                function(actor, state, params).into_internal()
            },
        );

        // Create the packet from the handler fn and params
        let packet = Packet::new_msg(handler_fn, function as usize, params);

        // Create a request from the packet and the reply
        Msg::new(&self.sender, packet)
    }

    pub(crate) fn new_msg_async<P: Send + 'static, F>(
        &'a self,
        function: AsyncMsgFn<A, P, F>,
        params: P,
    ) -> Msg<'a, A, P>
    where
        F: Future<Output = Flow<A>> + Send + 'static,
    {
        let handler_fn = HandlerFn::Async(
            |actor: &mut A, state: &mut <A as Actor>::State, handler_packet: HandlerPacket| {
                // transmute the fn_ptr
                let function: AsyncMsgFn<A, P, F> = unsafe { transmute(handler_packet.fn_ptr) };
                // downcast the packet data to params and the request
                let params = handler_packet.data.downcast_msg().unwrap();

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

    pub(crate) fn new_req<P: Send + 'static, R: Send + 'static>(
        &'a self,
        function: ReqFn<A, P, R>,
        params: P,
    ) -> Req<'a, A, P, R> {
        let handler_fn = HandlerFn::Sync(
            |actor: &mut A,
             state: &mut <A as Actor>::State,
             handler_packet: HandlerPacket|
             -> InternalFlow<A> {
                // transmute the fn_ptr
                let function: ReqFn<A, P, R> = unsafe { transmute(handler_packet.fn_ptr) };
                // downcast the packet data to params and the request
                let (params, request) = handler_packet.data.downcast_req().unwrap();
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

    pub(crate) fn new_req_async<P: Send + 'static, R: Send + 'static, F>(
        &'a self,
        function: AsyncReqFn<A, P, F>,
        params: P,
    ) -> Req<'a, A, P, R>
    where
        F: Future<Output = ReqFlow<A, R>> + Send + 'static,
    {
        let handler_fn = HandlerFn::Async(
            |actor: &mut A, state: &mut <A as Actor>::State, handler_packet: HandlerPacket| {
                // transmute the fn_ptr
                let function: AsyncReqFn<A, P, F> = unsafe { transmute(handler_packet.fn_ptr) };
                // downcast the packet data to params and the request
                let (params, request) = handler_packet.data.downcast_req().unwrap();

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
//  Address: implement traits
//--------------------------------------------------------------------------------------------------

impl<A: Actor> Clone for Address<A> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

mod test {
    use super::*;
    use crate::actor::ExitReason;
    use crate::callable::Callable;
    use crate::flows::{ExitFlow, InitFlow};
    use crate::state::State;
    use crate::{actor::Spawn, fun, sending::UnboundedSend};

    // #[tokio::test]
    async fn all_functions_dont_segfault_test() -> anyhow::Result<()> {
        let (child, address) = MyActor::spawn(MyActor);

        let res1 = address.new_msg(MyActor::test_a, 10).send()?;
        let res2 = address.new_msg_async(MyActor::test_b, 10).send()?;

        let res3 = address
            .new_req(MyActor::test_c, 10)
            .send()?
            .async_recv()
            .await?;
        let res4 = address
            .new_req_async(MyActor::test_d, 10)
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

        let res1 = child.call(MyActor::test_a, 10).send()?;
        let res1 = child.call(|actor, state, amount| {
            // actor.
            Flow::Ok
        }, 10).send()?;
        // let res1 = child.send(func!(MyActor::test_a2), ("hi", 10)).await?;

        let res2 = child.call(MyActor::test_b, 10).send()?;
        let res3 = child.call(MyActor::test_c, 10).send()?.await?;
        let res4 = address.call(MyActor::test_d, 10).send()?.await?;

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
    struct MyActor;

    impl MyActor {
        fn test_e(&mut self, c: u32) -> Flow<Self> {
            Flow::Ok
        }

        fn test_a(&mut self, state: &mut State<Self>, c: u32) -> Flow<Self> {
            Flow::Ok
        }

        fn test_a2(&mut self, s: &str, c: u32) -> Flow<Self> {
            Flow::Ok
        }

        async fn test_f<'r>(&'r mut self, c: u32) -> Flow<Self> {
            Flow::Ok
        }

        async fn test_b(&mut self, state: &mut State<Self>, c: u32) -> Flow<Self> {
            Flow::Ok
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
