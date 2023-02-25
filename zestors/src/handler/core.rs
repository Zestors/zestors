use crate as zestors;
use crate::all::*;
use async_trait::async_trait;

pub type HandlerInboxType<H> = <<H as Handler>::State as HandlerState<H>>::InboxType;
pub type HandlerProtocol<H> = <<H as Handler>::State as HandlerState<H>>::Protocol;
pub type HandlerInboxConfig<H> = <HandlerInboxType<H> as InboxType>::Config;

/// The  [`Handler`] trait is used together with the following four [async traits](async_trait) to
/// define the lifecycle of an actor:
/// - [`HandleMessage<M>`] --> Specifies how a message `M` is handled.
/// - [`HandleEvent`] --> Specifies how a [`HandlerEvent`] is handled. _([derivable](todo))_
/// - [`HandleExit`] --> Specifies how the actor's exit is handled. _([derivable](todo))_
/// - [`HandleInit<I>`] --> Specifies how the actor's initialization with `I` is handled. _([derivable](todo))_
pub trait Handler: Sized + Send + 'static {
    /// The exception that can be returned from any handler function.
    type Exception: Send + 'static;

    /// The state passed along to every handler function, usually the inbox.
    type State: HandlerState<Self>;
}

/// Specifies how the [`Handler`] handles a message `M`.
#[async_trait]
pub trait HandleMessage<M: Message>: Handler {
    async fn handle_msg(
        &mut self,
        state: &mut Self::State,
        msg: M::Payload,
    ) -> Result<Flow, Self::Exception>;
}

/// Specifies how the [`Handler`] handles a [`HandlerEvent`].
#[async_trait]
pub trait HandleEvent: Handler {
    async fn handle_event(
        &mut self,
        state: &mut Self::State,
        event: HandlerEvent,
    ) -> Result<Flow, Self::Exception> {
        match event {
            HandlerEvent::Halt => {
                state.close();
                Ok(Flow::Continue)
            }
            HandlerEvent::ClosedAndEmpty => Ok(Flow::Stop),
            HandlerEvent::Dead => Ok(Flow::Stop),
        }
    }
}

/// Specifies how the [`Handler`] handles it's exit.
#[async_trait]
pub trait HandleExit: Handler {
    /// The value that the process exits with.
    type Exit: Send + 'static;

    async fn handle_exit(
        self,
        state: &mut Self::State,
        exception: Option<Self::Exception>,
    ) -> HandlerExit<Self>;
}

/// Specifies how the [`Handler`] handles it's initialization.
#[async_trait]
pub trait HandleInit<I>: HandleExit {
    type Ref: Send + 'static;
    type StartError: Send + 'static;

    async fn init(
        child: Child<Self::Exit, HandlerInboxType<Self>>,
        address: Address<HandlerInboxType<Self>>,
    ) -> Result<(Child<Self::Exit, HandlerInboxType<Self>>, Self::Ref), Self::StartError>;

    async fn handle_init(init: I, state: &mut Self::State) -> Result<Self, Self::Exit>;
}

//------------------------------------------------------------------------------------------------
//  Auto traits
//------------------------------------------------------------------------------------------------

#[async_trait]
pub trait HandlerExt: Handler {
    async fn handle_protocol(
        &mut self,
        state: &mut Self::State,
        msg: HandlerProtocol<Self>,
    ) -> Result<Flow, Self::Exception> {
        msg.handle_with(self, state).await
    }
}
impl<T: Handler> HandlerExt for T {}

/// Specifies that a [`Protocol`] can be handled by the [`Handler`] `H`.
///
/// Normally this is automatically derived with [`protocol`]
#[async_trait]
pub trait HandledBy<H: Handler> {
    async fn handle_with(self, handler: &mut H, state: &mut H::State)
        -> Result<Flow, H::Exception>;
}

//------------------------------------------------------------------------------------------------
//  Additional
//------------------------------------------------------------------------------------------------

/// The value that a [`Handler::handle_stop`] can exit with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HandlerExit<H: Handler + HandleExit> {
    Exit(H::Exit),
    Resume(H),
}

/// The `Ok(_)` value of a handler-function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Flow {
    Continue,
    Stop,
}

//------------------------------------------------------------------------------------------------
//  Example
//------------------------------------------------------------------------------------------------

#[protocol]
enum TestProtocol {
    A(u32),
    B(String),
}

#[async_trait]
impl<H> HandledBy<H> for TestProtocol
where
    H: HandleMessage<u32> + HandleMessage<String>,
{
    async fn handle_with(
        self,
        handler: &mut H,
        state: &mut H::State,
    ) -> Result<Flow, H::Exception> {
        match self {
            TestProtocol::A(payload) => {
                HandleMessage::<u32>::handle_msg(handler, state, payload).await
            }
            TestProtocol::B(payload) => {
                HandleMessage::<String>::handle_msg(handler, state, payload).await
            }
        }
    }
}
