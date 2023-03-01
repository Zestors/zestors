use crate::all::*;
use async_trait::async_trait;

#[async_trait]
pub trait Handler: Sized + Send + 'static {
    /// The state that is passed to every handler function.
    ///
    /// This state contains the inbox of the process.
    type State: HandlerState<Self>;

    /// The `Err(T)` value that can be returned from any handler function.
    type Exception: Send;

    /// The [`Flow::Stop(T)`] value that can be returned from any handler function.
    type Stop: Send;

    /// The value that the process exits with.
    ///
    /// The [`Child`] returned when spawning a handler is a `Child<H::Exit, _>`.
    type Exit: Send + 'static;

    /// Describes how the [`Handler`] should handle an [`Event`]. The handler can either
    /// [`ExitFlow::Continue`] or [`ExitFlow::Exit`].
    async fn handle_exit(self, state: &mut Self::State, event: Event<Self>) -> ExitFlow<Self>;

    // async fn handle_event(
    //     &mut self,
    //     state: &mut Self::State,
    //     event: Event<Self>,
    // ) -> HandlerResult<Self> {
    //     todo!()
    // }
}

/// An event that the [`Handler`] should handle.
#[derive(Debug)]
pub enum Event<H: Handler> {
    /// This process has been halted and should exit.
    Halted,
    /// This process's inbox has been closed and is also empty.
    ClosedAndEmpty,
    /// Same as [`ClosedAndEmpty`], but no [`Action`]s have been scheduled either.
    /// This means that the actor won't do anything unless a new action is scheduled.
    Dead,
    /// A [`Handler::Exception`] was returned from a handler-function.
    Exception(H::Exception),
    /// A [`Flow::Stop`] was returned from a handler-function.
    Stop(H::Stop),
}

/// Specifies how the [`Handler`] handles the [`Message`] `M`.
#[async_trait]
pub trait HandleMessage<M: Message>: Handler {
    async fn handle_msg(&mut self, state: &mut Self::State, msg: M::Payload)
        -> HandlerResult<Self>;
}

/// Specifies how the [`Handler`] handles being started with `I`.
///
/// This trait is not mandatory since handlers can always be spawned with [`HandlerExt::spawn`],
/// but does offer much more flexibility; when [`HandleStart<I>`] is implemented [`HandlerExt::start::<I>`]
/// can be used in addition to [`HandlerExt::spawn`].
#[async_trait]
pub trait HandleStart<I>: Handler {
    /// The actor-reference returned, usually an [`Address<HandlersInbox<Self>>`].
    type Ref: Send + 'static;

    /// The error that can occur when starting. If no error can occur then use [`Infallible`]
    type StartError: Send + 'static;

    /// The initialization-sequence of the starting process.
    async fn initialize(
        address: Address<HandlerInbox<Self>>,
    ) -> Result<Self::Ref, Self::StartError>;

    /// The initialization-sequence of the starting process.
    async fn handle_start(init: I, state: &mut Self::State) -> Result<Self, Self::Exit>;
}

/// Specifies that a [`Protocol`] can be handled by the [`Handler`] `H`.
///
/// This is automatically implemented with the [`protocol`] macro.
#[async_trait]
pub trait HandledBy<H: Handler> {
    async fn handle_with(self, handler: &mut H, state: &mut H::State) -> HandlerResult<H>;
}

/// The return-value of the [`Handler::handle_exit`] function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExitFlow<H: Handler> {
    /// Exit with [`Handler::Exit`].
    Exit(H::Exit),
    /// Continue execution with `H`.
    Continue(H),
}

/// The `Ok(_)` value of a [`Handler`]-function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Flow<H: Handler> {
    /// Continue regular execution.
    Continue,
    /// Calls [`Handler::handle_exit`] with [`Event::Stop`].
    Stop(H::Stop),
    /// Immeadeately exits with [`Handler::Exit`].
    ExitDirectly(H::Exit),
}

/// Type-alias for the [`InboxType`] of the [`Handler`] `H`.
pub type HandlerInbox<H> = <<H as Handler>::State as HandlerState<H>>::InboxType;

/// Type-alias for the [`Protocol`] of the [`Handler`] `H`.
pub type HandlerProtocol<H> = <<H as Handler>::State as HandlerState<H>>::Protocol;

/// Type-alias for the [`InboxType::Config`] of the [`Handler`] `H`.
pub type HandlerConfig<H> = <HandlerInbox<H> as InboxType>::Config;

/// Type-alias for the return-value of a [`Handler`]-function.
pub type HandlerResult<H> = Result<Flow<H>, <H as Handler>::Exception>;
