use crate::{Actor, FullHandlerContext, HandledBy, HandlerContext};
use futures::future::ready;
use rootcause::{Report, report};
use std::{convert::Infallible, fmt::Debug};
use zestors_interface::{Interface, Message, ResolverOf};
use zestors_runtime::prelude::*;

/// A declarative and simple way to implement an [`Actor`], by providing a set of
/// lifecycle hooks and message handlers. Any type that implements [`Handler`]
/// implements [`Actor`] automatically.
///
/// # Lifecycle
/// 1. [`Handler::init`] is called when the actor is spawned, before any
///    message is handled.
/// 2. The actor then handles messages with its [`Handle<M>`] implementations,
///    and signals with [`Handler::on_shutdown`], [`Handler::on_suspend`] and
///    [`Handler::on_resume`].
/// 3. The loop ends when a [`Signal::Shutdown`] has been received and the
///    queued messages have been handled, or as soon as a handler or hook
///    returns an error. [`Handler::exit`] is then called with the reason, and
///    what it returns is the actor's result.
///
/// # Scheduling
/// During each event-loop, the actor polls the next signal to be received. If no signal is present,
/// the actor polls the next message or custom event to be handled. Once such an event is received,
/// the actor processes it, and then polls for the next event. As such, handler-methods should not
/// have long `.await` calls, as this will block the actor from processing messages and signals.
///
/// The [`Handler`] trait provides a [`next_event`](Handler::next_event) method that can be
/// overridden to schedule arbitrary futures or messages to be handled by the actor. The
/// [`BasicScheduler`](crate::BasicScheduler) can be used for this purpose, or a custom scheduler/stream can be implemented.
pub trait Handler: Debug + Sized + Send + 'static {
    /// The interface that this handler implements.
    ///
    /// The actor must implement [`Handle<M>`] for every message type `M` that it wants to handle. The [`Interface`] must additionally derive [`HandlerInterface`](derive@crate::HandlerInterface) (or implement it manually) to provide a way to handle messages of that type.
    type Interface: HandlerInterface<Self>;

    /// Called when the actor is spawned, before any message is handled.
    ///
    /// It runs alongside the inbox's signal receiver, so the actor's status can
    /// already be [`Running`](zestors_runtime::ActorStatus::Running) while
    /// `init` is still busy: `monitor_init` doesn't wait for it to finish.
    ///
    /// If a shutdown signal is received while this method is running, it is
    /// cancelled, and [`Handler::exit`] is called with
    /// [`HandlerExit::InitCancelled`].
    fn init(
        &mut self,
        _ctx: HandlerContext<'_, Self>,
    ) -> impl Future<Output = Result<(), Report>> + Send {
        async { Ok(()) }
    }

    /// Called when the actor is about to exit.
    ///
    /// This is the final lifecycle hook, called once when the event loop ends.
    /// The [`HandlerExit`] describes why. It is not called when the actor
    /// panics or is aborted.
    ///
    /// The default turns every reason but [`HandlerExit::Normal`] into an
    /// error.
    fn exit(
        &mut self,
        _ctx: HandlerContext<'_, Self>,
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
    /// [`Exiting`](zestors_runtime::ActorStatus::Exiting) state.
    ///
    /// If this method returns an error, [`Handler::exit`] will be called with [`HandlerExit::HandlerError`].
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
    /// [`Suspended`](zestors_runtime::ActorStatus::Suspended) state.
    ///
    /// If this method returns an error, [`Handler::exit`] will be called with [`HandlerExit::HandlerError`].
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
    /// [`Running`](zestors_runtime::ActorStatus::Running) state.
    ///
    /// If this method returns an error, [`Handler::exit`] will be called with [`HandlerExit::HandlerError`].
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
    /// For scheduling arbitrary futures or messages, see the [`BasicScheduler`](crate::BasicScheduler).
    /// Its `next` method can be returned from this method directly. The basic scheduler returns
    /// [`HandlerMessage`](crate::HandlerMessage)s, which can easily be created from any message
    /// type using [`HandlerMessage::new`](crate::HandlerMessage::new).
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
///
/// `req` is the message's reply channel: `()` for a fire-and-forget message,
/// and a [`Request<T>`](zestors_interface::Request) to reply with for a message
/// with `#[msg(reply = T)]`. Returning an error stops the actor.
pub trait Handle<M: Message>: Handler {
    /// Handles a message of type `M`.
    fn handle(
        &mut self,
        ctx: HandlerContext<'_, Self>,
        msg: M,
        req: ResolverOf<M>,
    ) -> impl Future<Output = Result<(), Report>> + Send;
}

impl<H: Handler> Handle<Infallible> for H {
    async fn handle(
        &mut self,
        _ctx: HandlerContext<'_, Self>,
        _msg: Infallible,
        _req: ResolverOf<Infallible>,
    ) -> Result<(), Report> {
        unreachable!("Infallible")
    }
}

/// Defines how an [`Interface`] can be handled by a [`Handler`].
///
/// This trait is derived using the [`HandlerInterface`](derive@crate::HandlerInterface)
/// derive macro, and is normally not implemented manually. It is required for
/// any interface that is used by a handler.
pub trait HandlerInterface<H: Handler>: Interface {
    /// Handle the interface using the provided [`Handler`].
    fn handle_with(
        self,
        ctx: HandlerContext<'_, H>,
        actor: &mut H,
    ) -> impl Future<Output = Result<(), Report>> + Send;
}

/// Describes the reason why a [`Handler`] is exiting.
///
/// This can be transformed into a `Result` using [`HandlerExit::into_result`].
#[derive(Debug)]
pub enum HandlerExit {
    /// The handler is exiting normally: after a shutdown signal, or because
    /// its inbox closed.
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
    /// `Ok(())` for [`HandlerExit::Normal`], and an error describing the
    /// reason otherwise.
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
        FullHandlerContext::new(state).run(&mut self).await
    }
}
