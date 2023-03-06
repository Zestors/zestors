use super::event_loop::event_loop;
use crate::all::*;
use async_trait::async_trait;

/// An extension to [`Handler`] with many useful functions.
#[async_trait]
pub trait HandlerExt: Handler {
    /// Run the event-loop of this [`Handler`].
    async fn run(self, state: Self::State) -> Self::Exit {
        event_loop(self, state).await
    }

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
            self.run(state).await
        })
    }

    fn spawn(
        self,
    ) -> (
        Child<Self::Exit, HandlerInbox<Self>>,
        Address<HandlerInbox<Self>>,
    ) {
        self.spawn_with(Self::default_link(), Self::default_config())
    }

    // fn start_with<'a, I>(
    //     init: I,
    //     link: Link,
    //     cfg: HandlerConfig<Self>,
    // ) -> BoxFuture<'a, Result<(Child<Self::Exit, HandlerInbox<Self>>, Self::Ref), Self::InitError>>
    // where
    //     Self: HandleInit<I> + 'a,
    // {
    //     Self::initialize(SpawnConfig::new(link, cfg), init)
    // }

    // fn start<'a, I>(
    //     init: I,
    // ) -> BoxFuture<'a, Result<(Child<Self::Exit, HandlerInbox<Self>>, Self::Ref), Self::InitError>>
    // where
    //     Self: HandleInit<I> + 'a,
    // {
    //     Self::initialize(SpawnConfig::default(), init)
    // }

    // fn create_spec<I>(init: I) -> HandlerSpec<Self, I>
    // where
    //     Self: HandleRestart<I>,
    //     I: Send + 'static,
    // {
    //     HandlerSpec::new(init)
    // }
}
impl<T: Handler> HandlerExt for T {}
