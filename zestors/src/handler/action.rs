use crate as zestors;
use crate::all::*;
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::{any::TypeId, fmt::Debug};

#[derive(Message)]
pub enum Action<H: Handler> {
    FnOnce(
        Box<
            dyn for<'a> FnOnce(
                    &'a mut H,
                    &'a mut H::State,
                ) -> BoxFuture<'a, Result<Flow, H::Exception>>
                + Send,
        >,
    ),
    Protocol(<H::State as HandlerState<H>>::Protocol),
}

impl<H> Debug for Action<H>
where
    H: Handler,
    <H::State as HandlerState<H>>::Protocol: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FnOnce(arg0) => f.debug_tuple("FnOnce").finish(),
            Self::Protocol(arg0) => f.debug_tuple("Protocol").finish(),
        }
    }
}

impl<H: Handler> Action<H> {
    pub fn from_fn<F>(function: F) -> Self
    where
        F: for<'a> FnOnce(&'a mut H, &'a mut H::State) -> BoxFuture<'a, Result<Flow, H::Exception>>
            + Send
            + 'static,
    {
        Self::FnOnce(Box::new(function))
    }

    pub fn from_protocol(protocol: <H::State as HandlerState<H>>::Protocol) -> Self {
        Self::Protocol(protocol)
    }

    pub fn from_msg<M: Message>(msg: M) -> (Self, M::Returned)
    where
        <H::State as HandlerState<H>>::Protocol: FromPayload<M>,
    {
        let (payload, returned) = msg.create();
        let action = Self::from_protocol(<<H::State as HandlerState<H>>::Protocol>::from_payload(
            payload,
        ));
        (action, returned)
    }

    pub async fn handle(self, handler: &mut H, state: &mut H::State) -> Result<Flow, H::Exception>
    where
        H: Handler,
    {
        match self {
            Action::FnOnce(function) => function(handler, state).await,
            Action::Protocol(protocol) => handler.handle_protocol(state, protocol).await,
        }
    }
}

impl<H: Handler> Protocol for Action<H> {
    fn into_boxed_payload(self) -> BoxPayload {
        BoxPayload::new::<Self>(self)
    }

    fn try_from_boxed_payload(payload: BoxPayload) -> Result<Self, BoxPayload>
    where
        Self: Sized,
    {
        payload.downcast::<Self>()
    }

    fn accepts_msg(msg_id: &TypeId) -> bool
    where
        Self: Sized,
    {
        *msg_id == TypeId::of::<Action<H>>()
    }
}

impl<H: Handler> FromPayload<Self> for Action<H> {
    fn from_payload(payload: Self) -> Self
    where
        Self: Sized,
    {
        payload
    }

    fn try_into_payload(self) -> Result<Self, Self>
    where
        Self: Sized,
    {
        Ok(self)
    }
}

#[async_trait]
impl<H: Handler> HandleMessage<Action<H>> for H {
    async fn handle_msg(
        &mut self,
        state: &mut Self::State,
        action: Action<H>,
    ) -> Result<Flow, Self::Exception> {
        action.handle(self, state).await
    }
}

macro_rules! action {
    (
        |$halter_ident:ident $(:$halter_ty:ty)?, $state_ident:ident $(:$state_ty:ty)?|
        async move $body:tt
    ) => {
        Action::from_fn(
            |$halter_ident $(:$halter_ty)?, $state_ident $(:$state_ty)?|
            Box::pin(async move $body)
        )
    };

    (
        |$halter_ident:ident $(:$halter_ty:ty)?, $state_ident:ident $(:$state_ty:ty)?|
        async $body:tt
    ) => {
        Action::from_fn(
            |$halter_ident $(:$halter_ty)?, $state_ident $(:$state_ty)?|
            Box::pin(async $body)
        )
    };

    ($function:path $(,$msg:expr)*) => {
        action!(|halter, state| async move {
            $function(halter, state $(,$msg)*).await
        })
    };
}

pub async fn my_message<'a, H: Handler>(
    handler: &'a mut H,
    state: &'a mut H::State,
    msg: u32,
    msg2: &'static str,
) -> Result<Flow, H::Exception> {
    Ok(Flow::Continue)
}

pub async fn my_message2<H: Handler>(
    handler: &mut H,
    state: &mut H::State,
) -> Result<Flow, H::Exception> {
    todo!()
}

pub async fn test<H: Handler>(address: Address<Inbox<Action<H>>>) {
    let val = String::from("hi");

    let _ = address
        .send(action!(|handler: &mut H, state| async move {
            let y = "hi";
            let val = val;
            state.close();
            Ok(Flow::Continue)
        }))
        .await;

    let _ = address.try_send(action!(|handler: &mut H, state| async {
        let y = "hi";
        todo!()
    }));

    let _ = address.force_send(action!(my_message::<H>, 10, "hi"));
    let _ = address.force_send(action!(my_message2::<H>));
}

// #[handler_envelope]
// impl MyHandler {
//     pub async fn say_hi(
//         &mut self,
//         state: &mut Self::State,
//         value: u32,
//     ) -> Result<Flow, Self::Exception> {
//         todo!()
//     }
//     pub async fn say_bye(
//         &mut self,
//         state: &mut Self::State,
//         msg: u32,
//         msg2: u64,
//     ) -> Result<Flow, Self::Exception> {
//         todo!()
//     }
// }
//
// Generates:
//
// pub trait MyHandlerEnvelope: ActorRef
// where
//     Self::ActorType: Accept<Action<MyHandler>>,
// {
//     fn say_hi(&self, value: u32) -> Envelope<'_, Self::ActorType, Action<MyHandler, (u32,)>> {
//         self.envelope(action!(MyHandler::say_hi, value))
//     }
//
//     fn say_bye(&self, msg: u32, msg2: u64) -> Envelope<'_, Self::ActorType, Action<MyHandler, (u32, u64,)>> {
//         self.envelope(action!(MyHandler::say_bye, msg, msg2))
//     }
// }

// This doesn't really work, since conversion between a dynamic action and a normal has a lot of overhead.
// Instead, it should probably be a manual FnOnce implementation of fn(_, _, Box<dyn Any>) + Box<dyn Any> that
// is downcast at runtime.
// This can be in addition to the normal FnOnce without Box<dyn Any> probably.
//
// pub struct Action2<H: Handler, T = Box<dyn Any + Send>> {
//     function: Box<
//         dyn for<'a> FnOnce(
//                 Option<(&'a mut H, &'a mut H::State)>,
//             ) -> Result<BoxFuture<'a, Result<Flow, H::Exception>>, T>
//             + Send,
//     >,
// }
//
// impl<H: Handler> Action2<H> {
//     pub fn from_fn_once<F>(function: F) -> Self
//     where
//         F: for<'a> FnOnce(&'a mut H, &'a mut H::State) -> BoxFuture<'a, Result<Flow, H::Exception>>
//             + Send
//             + 'static,
//     {
//         Self {
//             function: Box::new(|handler_and_state| match handler_and_state {
//                 Some((handler, state)) => {
//                     Ok(Box::pin(async move { function(handler, state).await }))
//                 }
//                 None => Err(Box::new(())),
//             }),
//         }
//     }
// }
//
// impl<H: Handler, T> Action2<H, T> {
//     pub async fn from_fn_once_with_params<F>(function: F, params: T) -> Self
//     where
//         F: for<'a> FnOnce(
//                 &'a mut H,
//                 &'a mut H::State,
//                 T,
//             ) -> BoxFuture<'a, Result<Flow, H::Exception>>
//             + Send
//             + 'static,
//         T: Send + 'static,
//     {
//         Self {
//             function: Box::new(|handler_and_state| match handler_and_state {
//                 Some((handler, state)) => {
//                     Ok(Box::pin(
//                         async move { function(handler, state, params).await },
//                     ))
//                 }
//                 None => Err(params),
//             }),
//         }
//     }
//
//     pub async fn handle(self, handler: &mut H, state: &mut H::State) -> Result<Flow, H::Exception> {
//         let Ok(future) = (self.function)(Some((handler, state))) else {
//             unreachable!()
//         };
//         future.await
//     }
//
//     pub fn cancel(self) -> T {
//         let Err(params) = (self.function)(None) else {
//             unreachable!()
//         };
//         params
//     }
// }
