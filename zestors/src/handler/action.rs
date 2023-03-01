use crate as zestors;
use crate::all::*;
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::{any::TypeId, fmt::Debug};

#[derive(Message)]
pub struct Action<H: Handler> {
    function: Box<
        dyn for<'a> FnOnce(
                &'a mut H,
                &'a mut H::State,
            ) -> BoxFuture<'a, Result<Flow<H>, H::Exception>>
            + Send,
    >,
}

impl<H: Handler> Action<H> {
    pub fn from_boxed_fn<F>(function: F) -> Self
    where
        F: for<'a> FnOnce(
                &'a mut H,
                &'a mut H::State,
            ) -> BoxFuture<'a, Result<Flow<H>, H::Exception>>
            + Send
            + 'static,
    {
        Self {
            function: Box::new(function),
        }
    }

    pub async fn handle_with(
        self,
        handler: &mut H,
        state: &mut H::State,
    ) -> Result<Flow<H>, H::Exception>
    where
        H: Handler,
    {
        (self.function)(handler, state).await
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

impl<H> Debug for Action<H>
where
    H: Handler,
    <H::State as HandlerState<H>>::Protocol: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Action").field("function", &"..").finish()
    }
}

#[async_trait]
impl<H: Handler> HandleMessage<Action<H>> for H {
    async fn handle_msg(
        &mut self,
        state: &mut Self::State,
        action: Action<H>,
    ) -> Result<Flow<H>, H::Exception> {
        action.handle_with(self, state).await
    }
}

#[macro_export]
/// A macro for easily creating actions from functions or closures.
macro_rules! action {
    (
        |$halter_ident:ident $(:$halter_ty:ty)?, $state_ident:ident $(:$state_ty:ty)?|
        async move $body:tt
    ) => {
        $crate::handler::Action::from_boxed_fn(
            |$halter_ident $(:$halter_ty)?, $state_ident $(:$state_ty)?|
            Box::pin(async move $body)
        )
    };

    (
        |$halter_ident:ident $(:$halter_ty:ty)?, $state_ident:ident $(:$state_ty:ty)?|
        async $body:tt
    ) => {
        $crate::handler::Action::from_boxed_fn(
            |$halter_ident $(:$halter_ty)?, $state_ident $(:$state_ty)?|
            Box::pin(async $body)
        )
    };

    ($function:path $(,$msg:expr)*) => {
        $crate::action!(|halter, state| async move {
            $function(halter, state $(,$msg)*).await
        })
    };
}
pub use action;

#[cfg(test)]
mod test {
    use super::*;

    pub async fn my_message<'a, H: Handler>(
        _handler: &'a mut H,
        _state: &'a mut H::State,
        _msg: u32,
        _msg2: &'static str,
    ) -> Result<Flow<H>, H::Exception> {
        Ok(Flow::Continue)
    }

    pub async fn my_message2<H: Handler>(
        _handler: &mut H,
        _state: &mut H::State,
    ) -> Result<Flow<H>, H::Exception> {
        todo!()
    }

    pub async fn test<H: Handler>(address: Address<Inbox<Action<H>>>) {
        let val = String::from("hi");

        let address = address.transform_into::<DynActor!(Action<H>)>();

        let _ = address
            .send(action!(|_handler: &mut H, state| async move {
                let _y = "hi";
                let _val = val;
                state.close();
                Ok(Flow::Continue)
            }))
            .await;

        let _ = address.try_send(action!(|_handler: &mut H, _state| async {
            let _y = "hi";
            todo!()
        }));

        let _ = address.force_send(action!(my_message::<H>, 10, "hi"));
        let _ = address.force_send(action!(my_message2::<H>));
    }
}
