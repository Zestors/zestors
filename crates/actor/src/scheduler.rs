use futures::{StreamExt as _, future::BoxFuture, stream::FuturesUnordered};
use rootcause::Report;
use zestors_codegen::Message;
use zestors_interface::{Resolver, prelude::*};

use crate::{Handle, Handler, HandlerContext};

/// A basic scheduler that allows scheduling futures to be run concurrently with the actor's message
/// processing. Add the scheduler to the actor's state, and call [`next`](Self::next) inside the
/// actor's [`next_event`](Handler::next_event) method to enable scheduling.
#[derive(Debug)]
pub struct BasicScheduler<H: Handler> {
    futures: FuturesUnordered<BoxFuture<'static, Result<Option<HandlerMessage<H>>, Report>>>,
}

impl<H: Handler> Default for BasicScheduler<H> {
    fn default() -> Self {
        Self::new()
    }
}

impl<H: Handler> BasicScheduler<H> {
    /// Creates a new, empty scheduler.
    pub fn new() -> Self {
        Self {
            futures: FuturesUnordered::new(),
        }
    }

    /// Schedule a future to be run concurrently with the actor's message processing, which produces a [`Message`] to be handled by the actor.
    pub fn schedule_msg<F, M>(&mut self, future_message: F)
    where
        F: Future<Output = Result<M, Report>> + Send + 'static,
        H: Handle<M>,
        M: Message,
    {
        self.futures.push(Box::pin(async move {
            Ok(Some(HandlerMessage::new(future_message.await?)))
        }));
    }

    /// Schedule a future to be run concurrently with the actor's message processing, which produces
    /// a callback to be executed in the context of the actor.
    pub fn schedule_callback<F>(&mut self, fut: impl Future<Output = F> + Send + 'static)
    where
        F: FnOnce(&mut H, HandlerContext<'_, H>) -> Result<(), Report> + Send + 'static,
    {
        self.futures.push(Box::pin(async move {
            let callback = fut.await;
            Ok(Some(HandlerMessage::new(HandlerCallback::new(callback))))
        }));
    }

    /// Schedule a future to be run concurrently with the actor's message processing.
    pub fn schedule_fut<F>(&mut self, future_message: F)
    where
        F: Future<Output = Result<(), Report>> + Send + 'static,
    {
        self.futures.push(Box::pin(async move {
            future_message.await?;
            Ok(None)
        }));
    }

    /// Polls the scheduler for the next completed future, returning a [`Message`] to be handled
    /// by the actor, or an error if the future failed.
    pub async fn next(&mut self) -> Option<Result<HandlerMessage<H>, Report>> {
        loop {
            match self.futures.next().await {
                Some(Ok(Some(msg))) => return Some(Ok(msg)),
                Some(Ok(None)) => continue,
                Some(Err(e)) => return Some(Err(e)),
                None => return None,
            }
        }
    }
}

/// A type-erased [`Message`], known to be handled by the [`Handler`] `H`.
#[derive(Message)]
#[zestors(interface_path = "zestors_interface")]
pub struct HandlerMessage<H: Handler> {
    msg: Box<dyn DynErasedMessage<H>>,
}

impl<H: Handler> HandlerMessage<H> {
    pub fn new<M>(msg: M) -> Self
    where
        H: Handle<M>,
        M: Message,
    {
        Self::from_box(Box::new(msg))
    }

    /// Same as [`HandlerMessage::new`], but for a message that is already boxed.
    pub fn from_box<M>(msg: Box<M>) -> Self
    where
        H: Handle<M>,
        M: Message,
    {
        Self { msg }
    }

    pub async fn handle(self, ctx: HandlerContext<'_, H>, actor: &mut H) -> Result<(), Report> {
        self.msg.handle_dyn(ctx, actor).await
    }
}

impl<H: Handler> Handle<HandlerMessage<H>> for H {
    async fn handle(
        &mut self,
        ctx: HandlerContext<'_, Self>,
        msg: HandlerMessage<H>,
        _req: <HandlerMessage<H> as Message>::Resolver,
    ) -> Result<(), Report> {
        msg.msg.handle_dyn(ctx, self).await
    }
}

impl<H: Handler> std::fmt::Debug for HandlerMessage<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandlerMessage")
            .field("msg", &"<dyn DynMessageHandledBy>")
            .finish()
    }
}

/// Internal trait for [`HandlerMessage`] to handle the message without knowing its concrete type.
trait DynErasedMessage<H: Handler>: Send + 'static {
    fn handle_dyn<'a>(
        self: Box<Self>,
        ctx: HandlerContext<'a, H>,
        actor: &'a mut H,
    ) -> BoxFuture<'a, Result<(), Report>>;
}

impl<M: Message, H: Handle<M>> DynErasedMessage<H> for M {
    fn handle_dyn<'a>(
        self: Box<Self>,
        ctx: HandlerContext<'a, H>,
        actor: &'a mut H,
    ) -> BoxFuture<'a, Result<(), Report>> {
        Box::pin(async move {
            let (resolver, receipt) = <M::Resolver as Resolver>::new();
            std::mem::drop(receipt);
            actor.handle(ctx, *self, resolver).await?;
            Ok(())
        })
    }
}

/// A message that can be used to send any arbitrary (synchronous) callback to a [`Handler`].
/// The callback is executed in the context of the handler, and can be used to perform
/// arbitrary actions on the handler's state.
///
/// This type is also useful for scheduling callbacks in the [`next_event`](Handler::next_event) method of a [`Handler`].
#[derive(Message)]
#[zestors(interface_path = "zestors_interface")]
pub struct HandlerCallback<H: Handler> {
    f: Box<dyn FnOnce(&mut H, HandlerContext<'_, H>) -> Result<(), Report> + Send + 'static>,
}

impl<H: Handler> HandlerCallback<H> {
    pub fn new(
        f: impl FnOnce(&mut H, HandlerContext<'_, H>) -> Result<(), Report> + Send + 'static,
    ) -> Self {
        Self { f: Box::new(f) }
    }
}

impl<H: Handler> Handle<HandlerCallback<H>> for H {
    async fn handle(
        &mut self,
        ctx: HandlerContext<'_, Self>,
        msg: HandlerCallback<H>,
        _req: (),
    ) -> Result<(), Report> {
        (msg.f)(self, ctx)
    }
}

impl<H: Handler> std::fmt::Debug for HandlerCallback<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandlerCallback")
            .field("f", &"<dyn FnOnce>")
            .finish()
    }
}

/// A trait for types that can be handled by a [`Handler`].
///
/// Automatically implemented for any type that implements [`Message`] and has a
/// corresponding [`Handle`] implementation for the given handler.
pub trait HandledBy<H: Handler>: Send + 'static {
    /// Handles `self`, constructing a fresh resolver/receipt pair and
    /// discarding the receipt since the caller has no way to wait on it.
    fn handle(
        self,
        ctx: HandlerContext<'_, H>,
        actor: &mut H,
    ) -> impl Future<Output = Result<(), Report>> + Send;
}

impl<H, M> HandledBy<H> for M
where
    H: Handle<M>,
    M: Message,
{
    fn handle(
        self,
        ctx: HandlerContext<'_, H>,
        actor: &mut H,
    ) -> impl Future<Output = Result<(), Report>> + Send {
        let (resolver, receipt) = <M::Resolver as Resolver>::new();
        std::mem::drop(receipt);
        actor.handle(ctx, self, resolver)
    }
}
