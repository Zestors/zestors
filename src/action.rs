use crate::{
    actor::Actor,
    flows::{EventFlow, MsgFlow, ReqFlow},
    function::{AnyFn, Params, MsgFn, ReqFn},
    messaging::Request,
};
use futures::Future;
use std::{fmt::Debug, marker::PhantomData, pin::Pin};

//--------------------------------------------------------------------------------------------------
//  helper types
//--------------------------------------------------------------------------------------------------



//------------------------------------------------------------------------------------------------
//  Action
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Action<A: ?Sized> {
    function: AnyFn,
    params: Params,
    a: PhantomData<A>,
}

impl<A: Actor> Action<A> {
    pub fn new<P: Send + 'static>(function: MsgFn<A, fn(P)>, params: P) -> Self {
        Self {
            function: function.into(),
            params: Params::new_msg(params),
            a: PhantomData,
        }
    }

    pub fn new_req<P: Send, R: Send>(
        function: ReqFn<A, fn(P) -> R>,
        params: P,
        request: Request<R>,
    ) -> Self {
        Self {
            function: function.into(),
            params: Params::new_req(params, request),
            a: PhantomData,
        }
    }

    pub unsafe fn transmute_req<P: Send, R: Send>(self) -> (P, Request<R>) {
        self.params.transmute_req::<P, R>()
    }

    pub fn downcast_msg<P: Send + 'static>(self) -> Result<P, Self> {
        match self.params.downcast_msg() {
            Ok(params) => Ok(params),
            Err(boxed) => Err(Self {
                function: self.function,
                params: unsafe { Params::new_raw(boxed) },
                a: PhantomData,
            }),
        }
    }

    pub async fn handle(self, actor: &mut A) -> EventFlow<A> {
        unsafe { self.function.call(actor, self.params).await }
    }

    pub(crate) unsafe fn new_raw_msg<P: Send + 'static>(function: AnyFn, params: P) -> Self {
        Self {
            function,
            params: Params::new_msg(params),
            a: PhantomData,
        }
    }

    pub(crate) unsafe fn new_raw_req<P: Send + 'static, R: Send + 'static>(
        function: AnyFn,
        params: P,
        request: Request<R>,
    ) -> Self {
        Self {
            function,
            params: Params::new_req(params, request),
            a: PhantomData,
        }
    }
}
