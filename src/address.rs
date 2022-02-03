use std::intrinsics::transmute;

use futures::Future;

use crate::{
    actor::Actor,
    callable::{
        MsgAsyncFnNostateType, MsgAsyncFnType, MsgFnNostateType, MsgFnType, ReqAsyncFnNostateType,
        ReqAsyncFnType, ReqFnNostateType, ReqFnType,
    },
    flow::{MsgFlow, ReqFlow},
    messaging::{Msg, PacketSender, Req},
    packets::{HandlerFn, HandlerPacket, Packet},
};

//-------------------------------------
// Address
//-------------------------------------

#[derive(Debug)]
pub struct Address<A: Actor + ?Sized> {
    pub(crate) sender: PacketSender<A>,
}

impl<A: Actor> Clone for Address<A> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<'a, 'b, A: Actor> Address<A> {
    pub fn msg<P: Send + 'static>(&'a self, function: MsgFnType<A, P>, params: P) -> Msg<'a, A, P> {
        let handler_fn = HandlerFn::Sync(
            |actor: &mut A,
             state: &mut <A as Actor>::State,
             handler_packet: HandlerPacket|
             -> MsgFlow<A> {
                // transmute the fn_ptr
                let function: MsgFnType<A, P> = unsafe { transmute(handler_packet.fn_ptr) };
                // downcast the packet data to params and the request
                let params = handler_packet.data.downcast_msg().unwrap();
                // call the function with everything necessary
                function(actor, state, params)
            },
        );

        // Create the packet from the handler fn and params
        let packet = Packet::new_msg(handler_fn, function as usize, params);

        // Create a request from the packet and the reply
        Msg::new(&self.sender, packet)
    }

    pub fn msg_async<P: Send + 'static, F>(
        &'a self,
        function: MsgAsyncFnType<A, P, F>,
        params: P,
    ) -> Msg<'a, A, P>
    where
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        let handler_fn = HandlerFn::Async(
            |actor: &mut A, state: &mut <A as Actor>::State, handler_packet: HandlerPacket| {
                // transmute the fn_ptr
                let function: MsgAsyncFnType<A, P, F> = unsafe { transmute(handler_packet.fn_ptr) };
                // downcast the packet data to params and the request
                let params = handler_packet.data.downcast_msg().unwrap();

                // now we transform the async function and box it
                Box::pin(async move {
                    // call and await the function
                    function(actor, state, params).await
                })
            },
        );

        // Create the packet from the handler fn and params
        let packet = Packet::new_msg(handler_fn, function as usize, params);

        // Create a request from the packet and the reply
        Msg::new(&self.sender, packet)
    }

    pub fn req<P: Send + 'static, R: Send + 'static>(
        &'a self,
        function: ReqFnType<A, P, R>,
        params: P,
    ) -> Req<'a, A, P, R> {
        let handler_fn = HandlerFn::Sync(
            |actor: &mut A,
             state: &mut <A as Actor>::State,
             handler_packet: HandlerPacket|
             -> MsgFlow<A> {
                // transmute the fn_ptr
                let function: ReqFnType<A, P, R> = unsafe { transmute(handler_packet.fn_ptr) };
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

    pub fn req_async<P: Send + 'static, R: Send + 'static, F>(
        &'a self,
        function: ReqAsyncFnType<A, P, F>,
        params: P,
    ) -> Req<'a, A, P, R>
    where
        F: Future<Output = ReqFlow<A, R>> + Send + 'static,
    {
        let handler_fn = HandlerFn::Async(
            |actor: &mut A, state: &mut <A as Actor>::State, handler_packet: HandlerPacket| {
                // transmute the fn_ptr
                let function: ReqAsyncFnType<A, P, F> = unsafe { transmute(handler_packet.fn_ptr) };
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

    pub fn msg_nostate<P: Send + 'static>(
        &'a self,
        function: MsgFnNostateType<A, P>,
        params: P,
    ) -> Msg<'a, A, P> {
        let handler_fn = HandlerFn::Sync(
            |actor: &mut A,
             _: &mut <A as Actor>::State,
             handler_packet: HandlerPacket|
             -> MsgFlow<A> {
                // transmute the fn_ptr
                let function: MsgFnNostateType<A, P> = unsafe { transmute(handler_packet.fn_ptr) };
                // downcast the packet data to params and the request
                let params = handler_packet.data.downcast_msg().unwrap();
                // call the function with everything necessary
                function(actor, params)
            },
        );

        // Create the packet from the handler fn and params
        let packet = Packet::new_msg(handler_fn, function as usize, params);

        // Create a request from the packet and the reply
        Msg::new(&self.sender, packet)
    }

    pub fn msg_async_nostate<P: Send + 'static, F>(
        &'a self,
        function: MsgAsyncFnNostateType<A, P, F>,
        params: P,
    ) -> Msg<'a, A, P>
    where
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        let handler_fn = HandlerFn::Async(
            |actor: &mut A, _: &mut <A as Actor>::State, handler_packet: HandlerPacket| {
                // transmute the fn_ptr
                let function: MsgAsyncFnNostateType<A, P, F> =
                    unsafe { transmute(handler_packet.fn_ptr) };
                // downcast the packet data to params and the request
                let params = handler_packet.data.downcast_msg().unwrap();

                // now we transform the async function and box it
                Box::pin(async move {
                    // call and await the function
                    function(actor, params).await
                })
            },
        );

        // Create the packet from the handler fn and params
        let packet = Packet::new_msg(handler_fn, function as usize, params);

        // Create a request from the packet and the reply
        Msg::new(&self.sender, packet)
    }

    pub fn req_nostate<P: Send + 'static, R: Send + 'static>(
        &'a self,
        function: ReqFnNostateType<A, P, R>,
        params: P,
    ) -> Req<'a, A, P, R> {
        let handler_fn = HandlerFn::Sync(
            |actor: &mut A,
             state: &mut <A as Actor>::State,
             handler_packet: HandlerPacket|
             -> MsgFlow<A> {
                // transmute the fn_ptr
                let function: ReqFnNostateType<A, P, R> =
                    unsafe { transmute(handler_packet.fn_ptr) };
                // downcast the packet data to params and the request
                let (params, request) = handler_packet.data.downcast_req().unwrap();
                // call the function with everything necessary
                let (msg_flow, reply) = function(actor, params).take_reply();
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

    pub fn req_async_nostate<P: Send + 'static, R: Send + 'static, F>(
        &'a self,
        function: ReqAsyncFnNostateType<A, P, F>,
        params: P,
    ) -> Req<'a, A, P, R>
    where
        F: Future<Output = ReqFlow<A, R>> + Send + 'static,
    {
        let handler_fn = HandlerFn::Async(
            |actor: &mut A, _: &mut <A as Actor>::State, handler_packet: HandlerPacket| {
                // transmute the fn_ptr
                let function: ReqAsyncFnNostateType<A, P, F> =
                    unsafe { transmute(handler_packet.fn_ptr) };
                // downcast the packet data to params and the request
                let (params, request) = handler_packet.data.downcast_req().unwrap();

                // now we transform the async function and box it
                Box::pin(async move {
                    // call and await the function
                    let (msg_flow, reply) = function(actor, params).await.take_reply();
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

mod test {
    use crate::{
        actor::{ActorExit, BasicState},
        callable::{Callable, Sendable},
        func,
    };

    use super::*;

    #[tokio::test]
    async fn all_functions_dont_segfault_test() -> anyhow::Result<()> {
        let child = MyActor.spawn();

        let res1 = child.msg(MyActor::test_a, 10).try_send()?;
        let res2 = child.msg_async(MyActor::test_b, 10).try_send()?;

        let res3 = child.req(MyActor::test_c, 10).try_send()?.recv().await?;
        let res4 = child
            .req_async(MyActor::test_d, 10)
            .try_send()?
            .recv()
            .await?;

        let res5 = child.msg_nostate(MyActor::test_e, 10).try_send()?;
        let res6 = child.msg_async_nostate(MyActor::test_f, 10).try_send()?;

        let res7 = child
            .req_nostate(MyActor::test_g, 10)
            .try_send()?
            .recv()
            .await?;
        let res8 = child
            .req_async_nostate(MyActor::test_h, 10)
            .try_send()?
            .recv()
            .await?;

        assert_eq!(res1, ());
        assert_eq!(res2, ());
        assert_eq!(res5, ());
        assert_eq!(res6, ());

        assert_eq!(res3, "ok");
        assert_eq!(res4, "ok");
        assert_eq!(res7, "ok");
        assert_eq!(res8, "ok");

        let res1 = child.send(func!(MyActor::test_a), 10).await?;
        let res2 = child.call(func!(MyActor::test_b), 10).send().await?;
        let res3 = child.try_send(func!(MyActor::test_c), 10)?.recv().await?;
        let res4 = child.try_send(func!(MyActor::test_d), 10)?.recv().await?;
        let res5 = child.call(func!(MyActor::test_e), 10).try_send()?;
        let res6 = child.try_send(func!(MyActor::test_f), 10)?;
        let res7 = child.try_send(func!(MyActor::test_g), 10)?.recv().await?;
        let res8 = child.try_send(func!(MyActor::test_h), 10)?.recv().await?;

        assert_eq!(res1, ());
        assert_eq!(res2, ());
        assert_eq!(res5, ());
        assert_eq!(res6, ());

        assert_eq!(res3, "ok");
        assert_eq!(res4, "ok");
        assert_eq!(res7, "ok");
        assert_eq!(res8, "ok");

        Ok(())
    }

    #[derive(Debug)]
    struct MyActor;

    impl MyActor {
        fn test_e(&mut self, c: u32) -> MsgFlow<Self> {
            MsgFlow::Ok
        }

        fn test_a(&mut self, state: &mut BasicState<Self>, c: u32) -> MsgFlow<Self> {
            MsgFlow::Ok
        }

        async fn test_f<'r>(&'r mut self, c: u32) -> MsgFlow<Self> {
            MsgFlow::Ok
        }

        async fn test_b(&mut self, state: &mut BasicState<Self>, c: u32) -> MsgFlow<Self> {
            MsgFlow::Ok
        }

        fn test_g(&mut self, c: u32) -> ReqFlow<Self, &'static str> {
            ReqFlow::Reply("ok")
        }

        fn test_c(&mut self, state: &mut BasicState<Self>, c: u32) -> ReqFlow<Self, &'static str> {
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
        type Error = anyhow::Error;
        type Exit = u32;
        type State = BasicState<Self>;

        fn starting(&mut self, state: &mut Self::State) {}

        fn stopping(self, state: &mut Self::State, exit: ActorExit<Self::Error>) -> Self::Exit {
            0
        }
    }
}
