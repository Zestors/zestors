use crate::*;
use futures::Future;
use std::sync::Arc;
use thiserror::Error;

//------------------------------------------------------------------------------------------------
//  InboxType
//------------------------------------------------------------------------------------------------

/// Anything that can be passed along as the argument to the spawn function.
pub trait Spawn: ActorRef + Send + 'static {
    type Config;

    /// Sets up the channel, preparing for x processes to be spawned.
    fn setup_channel(
        config: Self::Config,
        process_count: usize,
        address_count: usize,
        actor_id: ActorId,
    ) -> (
        Arc<<Self::ChannelDefinition as DefineChannel>::Channel>,
        Link,
    );

    /// Creates another inbox from the channel, without adding anything to its process count.
    fn new(channel: Arc<<Self::ChannelDefinition as DefineChannel>::Channel>) -> Self;
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
    S: Spawn,
    E: Send + 'static,
{
    let (channel, link) = S::setup_channel(config, 1, 1, ActorId::generate_new());
    let inbox = S::new(channel.clone());
    let handle = tokio::task::spawn(async move { function(inbox).await });
    (
        Child::new(channel.clone(), handle, link),
        Address::from_channel(channel),
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
    S: Spawn,
    E: Send + 'static,
{
    let (channel, link) = S::setup_channel(config, 1, 1, ActorId::generate_new());
    let inbox = S::new(channel.clone());
    let handle = tokio::task::spawn(async move { function(inbox).await });
    (
        Child::new(channel.clone(), vec![handle], link),
        Address::from_channel(channel),
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
    S: Spawn,
    E: Send + 'static,
    I: Send + 'static,
{
    let (channel, link) = S::setup_channel(config, iter.len(), 1, ActorId::generate_new());
    let handles = iter
        .map(|i| {
            let fun = function.clone();
            let inbox = S::new(channel.clone());
            tokio::task::spawn(async move { fun(i, inbox).await })
        })
        .collect::<Vec<_>>();
    (
        Child::new(channel.clone(), handles, link),
        Address::from_channel(channel),
    )
}

//------------------------------------------------------------------------------------------------
//  Errors
//------------------------------------------------------------------------------------------------

/// An error returned when trying to spawn more processes onto a channel
#[derive(Clone, PartialEq, Eq, Hash, Error)]
pub enum TrySpawnError<T> {
    /// The actor has exited.
    #[error("Couldn't spawn process because the actor has exited")]
    Exited(T),
    /// The spawned inbox does not have the correct type
    #[error("Couldn't spawn process because the given inbox-type is incorrect")]
    IncorrectType(T),
}

impl<T> std::fmt::Debug for TrySpawnError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exited(_) => f.debug_tuple("Exited").finish(),
            Self::IncorrectType(_) => f.debug_tuple("IncorrectType").finish(),
        }
    }
}

/// An error returned when spawning more processes onto a channel.
///
/// The actor has exited.
#[derive(Clone, PartialEq, Eq, Hash, Error)]
#[error("Couldn't spawn process because the channel has exited")]
pub struct SpawnError<T>(pub T);

impl<T> std::fmt::Debug for SpawnError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SpawnError").finish()
    }
}
