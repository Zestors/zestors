use crate::*;
use futures::Future;
use std::sync::Arc;

//------------------------------------------------------------------------------------------------
//  InboxType
//------------------------------------------------------------------------------------------------

/// Anything that can be passed along as the argument to the spawn function.
pub trait SpawnsWith: Sized + Send + 'static {
    type ChannelDefinition: DefinesChannel;
    type Config;

    /// Sets up the channel, preparing for x processes to be spawned.
    fn setup_channel(
        config: Self::Config,
        process_count: usize,
        address_count: usize,
    ) -> (
        Arc<<Self::ChannelDefinition as DefinesChannel>::Channel>,
        Link,
    );

    /// Creates another inbox from the channel, without adding anything to its process count.
    fn new(channel: Arc<<Self::ChannelDefinition as DefinesChannel>::Channel>) -> Self;
}

//------------------------------------------------------------------------------------------------
//  spawn
//------------------------------------------------------------------------------------------------

pub fn spawn<S, E, Fun, Fut>(
    config: S::Config,
    function: Fun,
) -> (
    Child<E, S::ChannelDefinition>,
    Address<S::ChannelDefinition>,
)
where
    Fun: FnOnce(S) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send,
    S: SpawnsWith,
    E: Send + 'static,
{
    let (channel, link) = S::setup_channel(config, 1, 1);
    let inbox = S::new(channel.clone());
    let handle = tokio::task::spawn(async move { function(inbox).await });
    (
        Child::new(channel.clone(), handle, link),
        Address::new(channel),
    )
}

pub fn spawn_one<S, E, Fun, Fut>(
    config: S::Config,
    function: Fun,
) -> (
    ChildPool<E, S::ChannelDefinition>,
    Address<S::ChannelDefinition>,
)
where
    Fun: FnOnce(S) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send,
    S: SpawnsWith,
    E: Send + 'static,
{
    let (channel, link) = S::setup_channel(config, 1, 1);
    let inbox = S::new(channel.clone());
    let handle = tokio::task::spawn(async move { function(inbox).await });
    (
        Child::new(channel.clone(), vec![handle], link),
        Address::new(channel),
    )
}

pub fn spawn_many<S, E, I, Fun, Fut>(
    iter: impl ExactSizeIterator<Item = I>,
    config: S::Config,
    function: Fun,
) -> (
    ChildPool<E, S::ChannelDefinition>,
    Address<S::ChannelDefinition>,
)
where
    Fun: FnOnce(I, S) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = E> + Send,
    S: SpawnsWith,
    E: Send + 'static,
    I: Send + 'static,
{
    let (channel, link) = S::setup_channel(config, iter.len(), 1);
    let handles = iter
        .map(|i| {
            let fun = function.clone();
            let inbox = S::new(channel.clone());
            tokio::task::spawn(async move { fun(i, inbox).await })
        })
        .collect::<Vec<_>>();
    (
        Child::new(channel.clone(), handles, link),
        Address::new(channel),
    )
}
