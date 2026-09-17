use crate::{Actor, FullHandlerState, HandledBy, HandlerState};
use futures::future::ready;
use rootcause::{Report, prelude::ResultExt, report};
use std::{convert::Infallible, fmt::Debug};
use zestors_interface::{Envelope, Interface, Message, Resolver};
use zestors_runtime::prelude::*;

/// A declarative and simple way to implement an [`Actor`], by providing a set of
/// lifecycle hooks and message handlers. Any type that implements [`Handler`]
/// implements [`Actor`] automatically.
///
/// # Lifecycle
/// 1. [`Handler::init`] is called when the actor is first spawned, before any messages are processed.
/// 2. The actor enters the [`ActorStatus::Running`] state, and begins processing messages and signals. Messages are handled using the [`Handle<M>`] trait, and signals are handled using the [`Handler::on_shutdown`], [`Handler::on_suspend`], and [`Handler::on_resume`] methods. If any of these methods return an error, the actor enters the [`ActorStatus::ShuttingDown`] state, and [`Handler::exit`] is called with [`ExitReason::HandlerError`].
/// 3. When a [`Signal::Shutdown`] is received, or if any handler or lifecycle hook
/// returns an error, the actor enters the [`ActorStatus::ShuttingDown`] state.
/// This then calls [`Handler::exit`] and exits the actor after all messages
/// have been processed.
///
/// # Scheduling
/// During each event-loop, the actor polls the next signal to be received. If no signal is present,
/// the actor polls the next message or custom event to be handled. Once such an event is received,
/// the actor processes it, and then polls for the next event. As such, handler-methods should not
/// have long `.await` calls, as this will block the actor from processing messages and signals.
///
/// The [`Handler`] trait provides a [`next_event`](Handler::next_event) method that can be
/// overridden to schedule arbitrary futures or messages to be handled by the actor. The
/// [`BasicScheduler`] can be used for this purpose, or a custom scheduler/stream can be implemented.
pub trait Handler: Debug + Sized + Send + 'static {
    /// The interface that this handler implements.
    ///
    /// The actor must implement [`Handle<M>`] for every message type `M` that it wants to handle. The [`Interface`] must additionally derive [`HandlerInterface`](derive@HandlerInterface) (or implement it manually) to provide a way to handle messages of that type.
    type Interface: HandlerInterface<Self>;

    /// Called when the actor is first spawned, before any messages are processed.
    /// This corresponds to [`ActorStatus::Initializing`].
    ///
    /// If a shutdown-signal is received while this method is running, this method
    /// will be cancelled, and [`Handler::exit`] will be called with
    /// [`ExitReason::InitCancelled`].
    fn init(
        &mut self,
        _state: HandlerState<'_, Self>,
    ) -> impl Future<Output = Result<(), Report>> + Send {
        async { Ok(()) }
    }

    /// Called when the actor is about to exit.
    ///
    /// This is the final lifecycle hook and is called exactly once, regardless
    /// of how the actor exits. The [`ExitReason`] describes why the actor is
    /// exiting.
    fn exit(
        &mut self,
        _state: HandlerState<'_, Self>,
        exit: HandlerExit,
    ) -> impl Future<Output = Result<(), Report>> + Send {
        async { exit.into_result() }
    }

    /// Called when the actor receives [`Signal::Shutdown`].
    ///
    /// The `Shutdown` signal automatically prevents the actor from receiving any
    /// new messages, and will cause the actor to exit after all messages have
    /// been processed. This method is only for performing any additional actions
    /// that may be needed when the actor is shutting down.
    ///
    /// When this method is called, the actor is already in the
    /// [`ActorStatus::ShuttingDown`] state.
    ///
    /// If this method returns an error, [`Handler::exit`] will be called with [`ExitReason::HandlerError`].
    ///
    /// This method should not have long `.await` calls, as it will block the
    /// actor from processing messages and signals.
    fn on_shutdown(
        &mut self,
        _address: &Address<Self::Interface>,
    ) -> impl Future<Output = Result<(), Report>> + Send {
        async { Ok(()) }
    }

    /// Called when the actor receives [`Signal::Suspend`].
    ///
    /// The `Suspend` signal automatically pauses the actor's message processing.
    /// This method is only for performing any additional actions that may be needed when the actor is suspended.
    ///
    /// When this method is called, the actor is already in the
    /// [`ActorStatus::Suspended`] state.
    ///
    /// If this method returns an error, [`Handler::exit`] will be called with [`ExitReason::HandlerError`].
    ///
    /// This method should not have long `.await` calls, as it will block the
    /// actor from processing messages and signals.
    fn on_suspend(
        &mut self,
        _address: &Address<Self::Interface>,
    ) -> impl Future<Output = Result<(), Report>> + Send {
        async { Ok(()) }
    }

    /// Called when the actor receives [`Signal::Resume`].
    ///
    /// The `Resume` signal automatically resumes the actor's message processing.
    /// This method is only for performing any additional actions that may be needed when the actor is resumed.
    ///
    /// When this method is called, the actor is already in the
    /// [`ActorStatus::Running`] state.
    ///
    /// If this method returns an error, [`Handler::exit`] will be called with [`ExitReason::HandlerError`].
    ///
    /// This method should not have long `.await` calls, as it will block the
    /// actor from processing messages and signals.
    fn on_resume(
        &mut self,
        _address: &Address<Self::Interface>,
    ) -> impl Future<Output = Result<(), Report>> + Send {
        async { Ok(()) }
    }

    /// Waits for the next event that should be processed.
    ///
    /// The default implementation returns `None`, which means the actor will only process
    /// messages and signals.
    ///
    /// The returned value has the following semantics: (similar to `Stream::next`)
    /// - `None` means the actor will not receive any more events from this source in this actor-loop.
    /// - `Some(Ok(M))` means the actor received an event `M` that should be processed.
    /// - `Some(Err(e))` means the actor received an error `e`, and the actor should exit.
    ///
    /// For scheduling arbitrary futures or messages, see the [`BasicScheduler`]. It's `next` method
    /// is compatible with this message. The basic scheduler returns [`ErasedMessage`]s, which can
    /// easily be created from any message type using [`ErasedMessage::new`].
    ///
    /// For scheduling a message at this instant, it's also possible to just send the message to the
    /// actor's own address, which will be processed normally like any other message.
    ///
    /// # Cancellation
    /// Any implementation **must** be cancellation-safe, since this future is cancelled whenever
    /// the actor receives a message or signal and starts processing it.
    fn next_event(
        &mut self,
    ) -> impl Future<Output = Option<Result<impl HandledBy<Self>, Report>>> + Send {
        ready::<Option<Result<Infallible, _>>>(None)
    }
}

/// Defines how a [`Handler`] handles a specific [`Message`].
pub trait Handle<M: Message>: Handler {
    /// Handles a message of type `M`.
    fn handle(
        &mut self,
        state: HandlerState<'_, Self>,
        env: Envelope<M>,
    ) -> impl Future<Output = Result<(), Report>> + Send;
}

// /// Defines how a [`Handler`] handles a specific [`Message`].
// pub trait HandleSimple<M: Message>: Handler {
//     /// Handles a message of type `M`.
//     fn handle(
//         &mut self,
//         state: HandlerState<'_, Self>,
//         msg: M,
//     ) -> impl Future<Output = Result<<M::Resolver as Resolver>::Input, Report>> + Send;
// }

// impl<T: HandleSimple<M>, M: Message> Handle<M> for T {
//     async fn handle(
//         &mut self,
//         state: HandlerState<'_, Self>,
//         Envelope { msg, req }: Envelope<M>,
//     ) -> Result<(), Report> {
//         let response = T::handle(self, state, msg).await.attach(format!(
//             "Failed to handle message of type {}",
//             std::any::type_name::<M>()
//         ))?;
//         if let Err(e) = req.resolve(response) {
//             tracing::warn!("Failed to resolve request: {e}");
//         }
//         Ok(())
//     }
// }

impl<H: Handler> Handle<Infallible> for H {
    fn handle(
        &mut self,
        _state: HandlerState<'_, Self>,
        _env: Envelope<Infallible>,
    ) -> impl Future<Output = Result<(), Report>> + Send {
        async { unreachable!("Infallible") }
    }
}

/// Defines how a [`Interface`] can be handled by a [`Handler`].
///
/// This trait is derived using the [`HandlerInterface`]
/// (derive@HandlerInterface) derive macro, and is normally not implemented
/// manually. It is required for any interface that us used by a handler.
pub trait HandlerInterface<H: Handler>: Interface {
    /// Handle the interface using the provided [`Handler`].
    fn handle_with(
        self,
        state: HandlerState<'_, H>,
        actor: &mut H,
    ) -> impl Future<Output = Result<(), Report>> + Send;
}

/// Describes the reason why a [`Handler`] is exiting.
///
/// This can be transformed into a `Result` using [`HandlerExit::into_result`].
#[derive(Debug)]
pub enum HandlerExit {
    /// The handler is exiting normally, due to a shutdown signal.
    Normal,

    /// The handler failed to initialize.
    InitError(Report),

    /// The handler was cancelled during initialization due to a shutdown signal.
    ///
    /// This means that the future returned by [`Handler::init`] was cancelled
    /// before it completed. The actor's state may be partially initialized. If
    /// necessary, the actor should handle this case in its [`Handler::exit`]
    /// method.
    InitCancelled,

    /// [`Handle<M>`] or a lifecycle hook returned an error, causing the handler
    /// to exit.
    HandlerError(Report),
}

impl HandlerExit {
    pub fn into_result(self) -> Result<(), Report> {
        match self {
            HandlerExit::Normal => Ok(()),
            HandlerExit::InitError(e) => Err(e.attach("handler initialization failed")),
            HandlerExit::InitCancelled => Err(report!(
                "handler initialization cancelled due to shutdown signal"
            )),
            HandlerExit::HandlerError(e) => Err(e.attach("handler encountered an error")),
        }
    }
}

impl From<HandlerExit> for Result<(), Report> {
    fn from(reason: HandlerExit) -> Self {
        reason.into_result()
    }
}

impl<H: Handler> Actor for H {
    type Interface = H::Interface;
    type Exit = ();

    async fn run(mut self, state: Inbox<Self::Interface>) -> Result<Self::Exit, Report> {
        FullHandlerState::new(state).run(&mut self).await
    }
}
