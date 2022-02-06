use std::{any::Any, pin::Pin};

use futures::Future;

use crate::{
    actor::Actor,
    flows::{InternalFlow, MsgFlow},
    messaging::{InnerRequest, Reply},
};

//-------------------------------------
// HandlerFn
//-------------------------------------

pub(crate) type HandlerFnSync<A> =
    unsafe fn(&mut A, &mut <A as Actor>::State, HandlerPacket) -> InternalFlow<A>;
pub(crate) type HandlerFnAsync<A> =
    for<'a> unsafe fn(
        &'a mut A,
        &'a mut <A as Actor>::State,
        HandlerPacket,
    ) -> Pin<Box<dyn Future<Output = InternalFlow<A>> + 'a + Send>>;

pub(crate) enum HandlerFn<A: Actor + ?Sized> {
    Sync(HandlerFnSync<A>),
    Async(HandlerFnAsync<A>),
}

//-------------------------------------
// Packet
//-------------------------------------

/// A single [Packet] which is sent to call a function remotely. It contains
/// a pointer to the correct handler function, as well as a [HandlerPacket].
pub struct Packet<A: Actor + ?Sized> {
    pub(crate) handler_fn: HandlerFn<A>,
    pub(crate) handler_packet: HandlerPacket,
}

impl<A: Actor> Packet<A> {
    pub(crate) async fn handle(self, actor: &mut A, state: &mut A::State) -> InternalFlow<A> {
        match self.handler_fn {
            HandlerFn::Sync(handler_fn) => unsafe { handler_fn(actor, state, self.handler_packet) },
            HandlerFn::Async(handler_fn) => unsafe {
                handler_fn(actor, state, self.handler_packet).await
            },
        }
    }
}

impl<A: Actor> std::fmt::Debug for Packet<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Packet")
            .field("handler_packet", &self.handler_packet)
            .finish()
    }
}

/// The [Packet] which is unpacked by the handler function. contains an untyped fn_pointer
/// with [PacketData]
#[derive(Debug)]
pub(crate) struct HandlerPacket {
    pub(crate) fn_ptr: usize,
    pub(crate) data: PacketData,
}

unsafe impl Send for HandlerPacket {}

/// Contains either just parameters, or parameters with an [InnerRequest]
#[derive(Debug)]
pub(crate) struct PacketData(Box<dyn Any>);

impl PacketData {
    pub(crate) fn new_msg<P: 'static + Send>(params: P) -> Self {
        Self(Box::new(params))
    }

    pub(crate) fn new_req<P: 'static + Send, R: 'static + Send>(
        params: P,
        request: InnerRequest<R>,
    ) -> Self {
        Self(Box::new((params, request)))
    }

    pub(crate) fn downcast_msg<P: 'static>(self) -> Result<P, Self> {
        match self.0.downcast() {
            Ok(params) => Ok(*params),
            Err(boxed) => Err(Self(boxed)),
        }
    }

    pub(crate) fn downcast_req<P: 'static, R: 'static>(self) -> Result<(P, InnerRequest<R>), Self> {
        match self.0.downcast() {
            Ok(params) => Ok(*params),
            Err(boxed) => Err(Self(boxed)),
        }
    }
}

impl<A: Actor> Packet<A> {
    pub(crate) fn new_msg<P: 'static + Send>(
        handler_fn: HandlerFn<A>,
        fn_ptr: usize,
        params: P,
    ) -> Self {
        Self {
            handler_fn,
            handler_packet: HandlerPacket {
                fn_ptr,
                data: PacketData::new_msg(params),
            },
        }
    }

    pub(crate) fn new_req<P: 'static + Send, R: 'static + Send>(
        handler_fn: HandlerFn<A>,
        fn_ptr: usize,
        params: P,
    ) -> (Self, Reply<R>) {
        let (req, reply) = InnerRequest::<R>::new();

        let packet = Self {
            handler_fn,
            handler_packet: HandlerPacket {
                fn_ptr,
                data: PacketData::new_req(params, req),
            },
        };

        (packet, reply)
    }

    pub fn get_params_req<P: 'static, R: 'static>(self) -> P {
        let (params, r): (P, InnerRequest<R>) = self.handler_packet.data.downcast_req().unwrap();
        params
    }

    pub fn get_params_msg<P: 'static>(self) -> P {
        self.handler_packet.data.downcast_msg().unwrap()
    }
}

unsafe impl<A: Actor> Send for Packet<A> {}
