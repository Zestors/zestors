use super::*;
use crate::all::*;
use async_trait::async_trait;

#[async_trait]
pub trait HandlerExt: Handler {
    async fn start_with<I>(
        init: I,
        link: Link,
        cfg: HandlerInboxConfig<Self>,
    ) -> Result<
        (
            Child<Self::Exit, <Self::State as HandlerState<Self>>::InboxType>,
            Self::Ref,
        ),
        Self::StartError,
    >
    where
        Self: HandleEvent + HandleInit<I>,
        I: Send + 'static,
    {
        event_loop::start_with::<Self, I>(init, link, cfg).await
    }

    async fn start<I>(
        init: I,
    ) -> Result<
        (
            Child<Self::Exit, <Self::State as HandlerState<Self>>::InboxType>,
            Self::Ref,
        ),
        Self::StartError,
    >
    where
        Self: HandleEvent + HandleInit<I>,
        I: Send + 'static,
        HandlerInboxConfig<Self>: Default,
    {
        event_loop::start_with::<Self, I>(init, Default::default(), Default::default()).await
    }
}
impl<T: Handler> HandlerExt for T {}
