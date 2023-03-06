use crate::all::*;
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::{any::TypeId, fmt::Debug};

/// An [`Action<H>`] can be handled by a [`Handler`] using [`Action::handle_with`]. Actions
/// can be created with the [`action!`] macro. Any `Handler` automatically implements
/// [`HandleMessage<Action<H>>`](HandleMessage). Any `Action<H>` implements [`Protocol`] and
/// [`Message`] automatically.
///
/// (See [`handler`](crate::handler) for an overview)
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
    /// Construct an action from a boxed [`FnOnce`].
    ///
    /// Normally an action is created with the [`action!`] macro.
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

    /// Handle the [`Action<H>`] with [`Handler`] `H`.
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

#[async_trait]
impl<H: Handler> HandledBy<H> for Action<H> {
    async fn handle_with(
        self,
        handler: &mut H,
        state: &mut <H as Handler>::State,
    ) -> HandlerResult<H> {
        Action::handle_with(self, handler, state).await
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

/// A macro for creating [`Action`]s.
///
/// # Usage
/// ```
/// use zestors::{prelude::*, action, Handler};
///
/// // If we define a handler ..
/// #[derive(Handler)]
/// #[state(Inbox<Action<MyHandler>>)]
/// struct MyHandler;
///
/// // .. we can create actions with:
/// let _action = action!(|_handler: &mut MyHandler, _state| async move {
///     Ok(Flow::Continue)
/// });
///
/// // And if we define some handler functions ..
/// impl MyHandler {
///     pub async fn handler_fn_1(
///         &mut self,
///         _state: &mut Inbox<Action<MyHandler>>,
///     ) -> HandlerResult<Self> {
///         Ok(Flow::Continue)
///     }
///
///     pub async fn handler_fn_2(
///         &mut self,
///         _state: &mut Inbox<Action<MyHandler>>,
///         _param1: u32,
///         _param2: String,
///     ) -> HandlerResult<Self> {
///         Ok(Flow::Continue)
///     }
/// }
///
/// // .. we can create actions with:
/// let _action = action!(MyHandler::handler_fn_1);
/// let _action = action!(MyHandler::handler_fn_2, 10, String::from("hi"));
/// ```
#[macro_export]
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
    use zestors_codegen::Handler;
    use super::*;

    #[derive(Handler)]
    #[state(Inbox<Action<MyHandler>>)]
    struct MyHandler {
        inner: u32,
    }

    impl MyHandler {
        pub async fn handler_fn_1(
            &mut self,
            _state: &mut Inbox<Action<MyHandler>>,
        ) -> HandlerResult<Self> {
            println!("Exectuted handler_fn_1!");
            Ok(Flow::Continue)
        }

        pub async fn handler_fn_2(
            &mut self,
            _state: &mut Inbox<Action<MyHandler>>,
            param1: u32,
            param2: String,
        ) -> HandlerResult<Self> {
            self.inner += param1;
            println!("handler_fn_2: {param2}");
            Ok(Flow::Continue)
        }
    }

    #[tokio::test]
    async fn compile_test() {
        let (mut child, address) = MyHandler { inner: 0 }.spawn();

        let outside_value = String::from("hi");
        address
            .send(action!(|handler: &mut MyHandler, _state| async move {
                println!("We can access the handler's state: {}", handler.inner);
                println!("And also from outside the closure: {}", outside_value);
                Ok(Flow::Continue)
            }))
            .await
            .unwrap();

        address
            .send(action!(MyHandler::handler_fn_1))
            .await
            .unwrap();

        address
            .send(action!(
                MyHandler::handler_fn_2,
                10,
                String::from("param_2")
            ))
            .await
            .unwrap();

        child.shutdown().await.unwrap().unwrap();
    }
}
