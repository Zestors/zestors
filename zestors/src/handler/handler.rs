use crate::all::*;
use async_trait::async_trait;

/// The [`Handler`] trait makes it easy to write actors in a declarative manner. See [`HandlerExt`] for
/// methods to call on a handler.
///
/// (See [`handler`](crate::handler) for an overview).
#[async_trait]
pub trait Handler: Sized + Send + 'static {
    /// The state is passed to every handler function and always contains the inbox.
    type State: HandlerState<Self>;

    /// The `Err(T)` value that can be returned from any handler function.
    type Exception: Send;

    /// The [`Flow::Stop(T)`] value that can be returned from any handler function.
    type Stop: Send;

    /// The value that the process exits with.
    ///
    /// The [`Child`] returned when spawning a handler is a `Child<H::Exit, _>`.
    type Exit: Send + 'static;

    /// Specifies how the [`Handler`] handles it's exit. The handler can either
    /// [`ExitFlow::Continue`] or [`ExitFlow::Exit`].
    async fn handle_exit(
        self,
        state: &mut Self::State,
        reason: Result<Self::Stop, Self::Exception>,
    ) -> ExitFlow<Self>;

    /// Specifies how the [`Handler`] handles an [`Event`] generated by it's [`HandlerState`].
    async fn handle_event(&mut self, state: &mut Self::State, event: Event) -> HandlerResult<Self>;

    /// The default [`Link`] used when this process is spawned or started.
    fn default_link() -> Link {
        Default::default()
    }

    /// The default [`InboxType::Config`] used when this process is spawned or started.
    fn default_config() -> HandlerConfig<Self> {
        Default::default()
    }
}

/// Specifies how the [`Handler`] handles the [`Message`] `M`.
#[async_trait]
pub trait HandleMessage<M: Message>: Handler {
    async fn handle_msg(&mut self, state: &mut Self::State, msg: M::Payload)
        -> HandlerResult<Self>;
}

pub enum RestartReason<E, Err> {
    InitError(Err),
    Exit(Result<E, ExitError>),
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
    /// Calls [`Handler::handle_exit`] with `Ok(H::Stop)`.
    Stop(H::Stop),
    /// Immeadeately exits with [`Handler::Exit`] without calling [`Handler::handle_exit`].
    ExitDirectly(H::Exit),
}

/// An event generated by the [`HandlerState`] that the [`Handler`] should handle.
#[derive(Debug)]
pub enum Event {
    /// This process has been halted and should exit.
    Halted,
    /// This process's inbox has been closed and is also empty.
    ClosedAndEmpty,
    /// Same as `ClosedAndEmpty` but with the extra guarantee that no [`Action`]s have
    /// been scheduled either.
    ///
    /// This means that the actor won't do anything until a new action is scheduled. If the process
    /// neither exits nor schedules a new future, it will panic!
    Dead,
}

impl Event {
    /// Handle the [`Event`] with [`Handler`]
    pub async fn handle_with<H: Handler>(
        self,
        handler: &mut H,
        state: &mut H::State,
    ) -> HandlerResult<H> {
        handler.handle_event(state, self).await
    }
}

/// Type-alias for the [`InboxType`] of the [`Handler`] `H`.
pub type HandlerInbox<H> = <<H as Handler>::State as HandlerState<H>>::InboxType;

/// Type-alias for the [`Protocol`] of the [`Handler`] `H`.
pub type HandlerProtocol<H> = <<H as Handler>::State as HandlerState<H>>::Protocol;

/// Type-alias for the [`InboxType::Config`] of the [`Handler`] `H`.
pub type HandlerConfig<H> = <HandlerInbox<H> as InboxType>::Config;

/// Type-alias for the return-value of a [`Handler`]-function.
pub type HandlerResult<H> = Result<Flow<H>, <H as Handler>::Exception>;

// pub struct SpawnConfig<H: Handler> {
//     link: Link,
//     cfg: HandlerConfig<H>,
// }

// impl<H: Handler> SpawnConfig<H> {
//     pub(crate) fn new(link: Link, cfg: HandlerConfig<H>) -> Self {
//         Self { link, cfg }
//     }

//     pub(crate) fn default() -> Self {
//         Self {
//             link: H::default_link(),
//             cfg: H::default_config(),
//         }
//     }

//     pub fn spawn(self, handler: H) -> (Child<H::Exit, HandlerInbox<H>>, Address<HandlerInbox<H>>) {
//         spawn_with(self.link, self.cfg, |inbox| async move {
//             handler
//                 .run(<H::State as HandlerState<H>>::from_inbox(inbox))
//                 .await
//         })
//     }

//     pub fn spawn_initialize<F>(
//         self,
//         init_fn: F,
//     ) -> (Child<H::Exit, HandlerInbox<H>>, Address<HandlerInbox<H>>)
//     where
//         F: for<'a> FnOnce(&'a mut H::State) -> BoxFuture<'a, Result<H, H::Exit>> + Send + 'static,
//     {
//         spawn_with(self.link, self.cfg, |inbox| async move {
//             let mut state = <H::State as HandlerState<H>>::from_inbox(inbox);
//             match init_fn(&mut state).await {
//                 Ok(handler) => handler.run(state).await,
//                 Err(exit) => exit,
//             }
//         })
//     }
// }

// #[async_trait]
// pub trait HandleInit<I>: Handler {
//     type Ref: Send + 'static;
//     type InitError: Error + Send;

//     /// Asynchronously spawn and initialize the process with `I`. The [`SpawnConfig`]
//     /// can be used either with [`SpawnConfig::spawn`] or [`SpawnConfig::spawn_initialize`].
//     ///
//     async fn initialize(
//         setup: SpawnConfig<Self>,
//         init: I,
//     ) -> Result<(Child<Self::Exit, HandlerInbox<Self>>, Self::Ref), Self::InitError>;
// }