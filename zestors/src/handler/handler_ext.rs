use crate::all::*;
use async_trait::async_trait;

/// An extension to [`Handler`] with many useful functions.
#[async_trait]
pub trait HandlerExt: Handler {
    /// An asynchronous and fallible alternative to [`Self::spawn`]. 
    /// 
    /// [`Handler`]s have to implement [`HandleStart<I>`] to use this.
    async fn start<I: Send + 'static>(
        init: I,
    ) -> Result<(Child<Self::Exit, HandlerInbox<Self>>, Self::Ref), Self::StartError>
    where
        Self: HandleStart<I>,
        HandlerConfig<Self>: Default,
    {
        Self::start_with(init, Default::default(), Default::default()).await
    }

    /// An asynchronous and fallible alternative to [`Self::spawn_with`]. 
    /// 
    /// [`Handler`]s have to implement [`HandleStart<I>`] to use this.
    async fn start_with<I: Send + 'static>(
        init: I,
        link: Link,
        cfg: HandlerConfig<Self>,
    ) -> Result<(Child<Self::Exit, HandlerInbox<Self>>, Self::Ref), Self::StartError>
    where
        Self: HandleStart<I>,
    {
        let (child, address) = spawn_with(link, cfg, |inbox| async move {
            let mut state = <Self::State as HandlerState<Self>>::from_inbox(inbox);

            match Self::handle_start(init, &mut state).await {
                Ok(handler) => run(handler, state).await,
                Err(exit) => exit,
            }
        });

        Ok((child, Self::initialize(address).await?))
    }

    /// [`crate::spawning::spawn`] for a [`Handler`].
    fn spawn(
        self,
    ) -> (
        Child<Self::Exit, HandlerInbox<Self>>,
        Address<HandlerInbox<Self>>,
    )
    where
        HandlerConfig<Self>: Default,
    {
        self.spawn_with(Default::default(), Default::default())
    }

    /// [`crate::spawning::spawn_with`] for a [`Handler`].
    fn spawn_with(
        self,
        link: Link,
        cfg: HandlerConfig<Self>,
    ) -> (
        Child<Self::Exit, HandlerInbox<Self>>,
        Address<HandlerInbox<Self>>,
    ) {
        spawn_with(link, cfg, |inbox| async move {
            let state = <Self::State as HandlerState<Self>>::from_inbox(inbox);
            run(self, state).await
        })
    }

    /// [`crate::spawning::spawn_many`] for a [`Handler`].
    fn spawn_many(
        self,
        amount: usize,
    ) -> (
        ChildPool<Self::Exit, HandlerInbox<Self>>,
        Address<HandlerInbox<Self>>,
    )
    where
        Self: Clone,
        HandlerInbox<Self>: MultiProcessInbox,
        HandlerConfig<Self>: Default,
    {
        self.spawn_many_with(Default::default(), Default::default(), amount)
    }

    /// [`crate::spawning::spawn_many_with`] for a [`Handler`].
    fn spawn_many_with(
        self,
        link: Link,
        cfg: HandlerConfig<Self>,
        amount: usize,
    ) -> (
        ChildPool<Self::Exit, HandlerInbox<Self>>,
        Address<HandlerInbox<Self>>,
    )
    where
        Self: Clone,
        HandlerInbox<Self>: MultiProcessInbox,
    {
        spawn_many_with(link, cfg, 0..amount, |_, inbox| async move {
            let state = <Self::State as HandlerState<Self>>::from_inbox(inbox);
            run(self, state).await
        })
    }

    /// Run the event-loop of this [`Handler`].
    async fn run(self, state: Self::State) -> Self::Exit {
        run(self, state).await
    }
}
impl<T: Handler> HandlerExt for T {}

/// Runs the event-loop of the handler.
async fn run<H: Handler>(mut handler: H, mut state: H::State) -> H::Exit {
    loop {
        let event_flow = match state.next_handler_item().await {
            HandlerItem::Dead => handler.handle_exit(&mut state, Event::Dead).await,
            HandlerItem::ClosedAndEmpty => {
                handler
                    .handle_exit(&mut state, Event::ClosedAndEmpty)
                    .await
            }
            HandlerItem::Halted => handler.handle_exit(&mut state, Event::Halted).await,
            HandlerItem::Action(action) => {
                match action.handle_with(&mut handler, &mut state).await {
                    Ok(Flow::Continue) => ExitFlow::Continue(handler),
                    Ok(Flow::ExitDirectly(exit)) => ExitFlow::Exit(exit),
                    Ok(Flow::Stop(stop)) => {
                        handler.handle_exit(&mut state, Event::Stop(stop)).await
                    }
                    Err(exception) => {
                        handler
                            .handle_exit(&mut state, Event::Exception(exception))
                            .await
                    }
                }
            }
            HandlerItem::Protocol(protocol) => {
                match protocol.handle_with(&mut handler, &mut state).await {
                    Ok(Flow::Continue) => ExitFlow::Continue(handler),
                    Ok(Flow::ExitDirectly(exit)) => ExitFlow::Exit(exit),
                    Ok(Flow::Stop(stop)) => {
                        handler.handle_exit(&mut state, Event::Stop(stop)).await
                    }
                    Err(exception) => {
                        handler
                            .handle_exit(&mut state, Event::Exception(exception))
                            .await
                    }
                }
            }
        };

        handler = match event_flow {
            ExitFlow::Exit(exit) => break exit,
            ExitFlow::Continue(handler) => handler,
        }
    }
}
