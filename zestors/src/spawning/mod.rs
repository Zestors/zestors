use crate::*;
use futures::Future;
use zestors_core::{inbox::InboxKind};
use thiserror::Error;

//------------------------------------------------------------------------------------------------
//  spawn
//------------------------------------------------------------------------------------------------

pub fn spawn<I, E, Fun, Fut>(config: I::Cfg, function: Fun) -> (Child<E, I>, Address<I>)
where
    Fun: FnOnce(I) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: InboxKind,
    E: Send + 'static,
{
    let (channel, link) = I::setup_channel(config, 1, 1, ActorId::generate());
    let inbox = I::new(channel.clone());
    let handle = tokio::task::spawn(async move { function(inbox).await });
    (
        Child::new(channel.clone(), handle, link),
        Address::from_channel(channel),
    )
}

pub fn spawn_one<I, E, Fun, Fut>(config: I::Cfg, function: Fun) -> (ChildGroup<E, I>, Address<I>)
where
    Fun: FnOnce(I) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: InboxKind,
    E: Send + 'static,
{
    let (channel, link) = I::setup_channel(config, 1, 1, ActorId::generate());
    let inbox = I::new(channel.clone());
    let handle = tokio::task::spawn(async move { function(inbox).await });
    (
        Child::new(channel.clone(), vec![handle], link),
        Address::from_channel(channel),
    )
}

pub fn spawn_many<I, E, Itm, Fun, Fut>(
    iter: impl ExactSizeIterator<Item = Itm>,
    config: I::Cfg,
    function: Fun,
) -> (ChildGroup<E, I>, Address<I>)
where
    Fun: FnOnce(Itm, I) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: InboxKind,
    E: Send + 'static,
    Itm: Send + 'static,
{
    let (channel, link) = I::setup_channel(config, iter.len(), 1, ActorId::generate());
    let handles = iter
        .map(|i| {
            let fun = function.clone();
            let inbox = I::new(channel.clone());
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
