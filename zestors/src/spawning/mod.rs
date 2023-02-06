/*!

# Spawn functions
When spawning an actor, there are a couple of options to choose from:
- [`spawn(f)`](spawn) - This is the simplest way to spawn an actor, and uses the default [`Link`] and
[`InboxType::Config`] to spawn function `f`.
- [`spawn_with(link, cfg, f)`](spawn_with) - Same as `spawn`, but allows for a
custom link and config.
- [`spawn_group(iter, f)`](spawn_group) - Same as `spawn`, but instead spawns many processes using
the iterator as the first argument to `f`.
- [`spawn_group_with(iter, link, cfg, f)`](spawn_group_with) - Same as `spawn_group`, but allows for a
custom link and config.

The inbox's config can be different for any [`InboxType`]. A [`Halter`] has `()` as it's config,
while an [`Inbox`] uses a [`Capacity`].

## Spawning onto an actor that is running
It is also possible to spawn more processes onto an actor that is already running using
[`ChildGroup::spawn_onto`] and [`ChildGroup::try_spawn_onto`]. These methods will fail if the actor has
already exited.

# Link
Every actor that is spawned must be spawned with a [`Link`] that indicates whether the actor is attached
or detached. By default a [`Link`] is attached with an abort-timer of 1 second; this means that when the
[`Child`] is dropped, the actor has 1 second to halt before it is aborted. If the link is detached, then
the child can be dropped without halting or aborting the actor.

| __<--__ [`actor_type`] | [`inboxes`] __-->__ |
|---|---|

# Example
```
    use std::time::Duration;
    use zestors::{inboxes::inbox::RecvError, *};
    use tokio::time::sleep;

    async fn inbox_actor(mut inbox: Inbox<()>) {
        loop {
            if let Err(RecvError::Halted) = inbox.recv().await {
                break ();
            }
        }
    }

    async fn halter_actor(halter: MultiHalter) {
        halter.await
    }

    # tokio_test::block_on(main());
    async fn main() {
        // When we spawn a `Halter`, we give it `()` as config...
        let _ = spawn_with(Link::default(), (), halter_actor);
        // while an `Inbox` takes `Capacity`.
        let _ = spawn_with(Link::default(), Capacity::default(), inbox_actor);

        // Now if we spawn a basic actor with defaults...
        let (child, address) = spawn(inbox_actor);
        drop(child);
        sleep(Duration::from_millis(10)).await;
        // when the child is dropped, the actor is aborted.
        assert!(address.has_exited());

        // But if we spawn a child that is not linked...
        let (child, address) = spawn_with(Link::Detached, Capacity::default(), inbox_actor);
        drop(child);
        sleep(Duration::from_millis(10)).await;
        // the actor does not get halted
        assert!(!address.has_exited());

        // It is also possible to spawn a process-group using an `Inbox`...
        let _ = spawn_group(0..10, |i, inbox| async move {
            println!("Spawning process nr {i}");
            inbox_actor(inbox).await
        });
        // or a `Halter`...
        let _ = spawn_group(0..10, |i, halter| async move {
            println!("Spawning process nr {i}");
            halter_actor(halter).await
        });
        // or either one using a custom config.
        let (mut child_group, address) = spawn_group_with(
            Link::Attached(Duration::from_millis(100)),
            Capacity::default(),
            0..10,
            |i, inbox| async move {
                println!("Spawning process nr {i}");
                inbox_actor(inbox).await
            },
        );

        // Since we have a child_group, it is possible to spawn more processes onto
        // the actor:
        child_group.spawn_onto(inbox_actor).unwrap();
        child_group.spawn_onto(inbox_actor).unwrap();

        // This can even be done if the child_group is dynamic:
        let mut child_group = child_group.into_dyn();
        child_group.try_spawn_onto(inbox_actor).unwrap();
        child_group.try_spawn_onto(inbox_actor).unwrap();

        // But fails if we spawn an actor with a different inbox-type:
        child_group.try_spawn_onto(halter_actor).unwrap_err();
    }
```
 */

use crate::*;
use futures::Future;
use std::{
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
    time::Duration,
};
use thiserror::Error;

pub use zestors_core::spawning::*;

//------------------------------------------------------------------------------------------------
//  Link
//------------------------------------------------------------------------------------------------

/// This decides whether the actor is attached or detached. If it is attached, then the
/// abort-timer is specified here as well.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Link {
    Detached,
    Attached(Duration),
}

impl Link {
    pub fn attach(&mut self, mut duration: Duration) -> Option<Duration> {
        match self {
            Link::Detached => {
                *self = Link::Attached(duration);
                None
            }
            Link::Attached(old_duration) => {
                std::mem::swap(old_duration, &mut duration);
                Some(duration)
            }
        }
    }

    pub fn detach(&mut self) -> Option<Duration> {
        match self {
            Link::Detached => {
                *self = Link::Detached;
                None
            }
            Link::Attached(_) => {
                let mut link = Link::Detached;
                std::mem::swap(self, &mut link);
                match link {
                    Link::Attached(duration) => Some(duration),
                    Link::Detached => unreachable!(),
                }
            }
        }
    }

    /// Whether the link is attached.
    pub fn is_attached(&self) -> bool {
        matches!(self, Link::Attached(_))
    }
}

impl Default for Link {
    fn default() -> Self {
        Link::Attached(get_default_abort_timer())
    }
}

//------------------------------------------------------------------------------------------------
//  Default abort timer
//------------------------------------------------------------------------------------------------

static DEFAULT_ABORT_TIMER_NANOS: AtomicU32 = AtomicU32::new(0);
static DEFAULT_ABORT_TIMER_SECS: AtomicU64 = AtomicU64::new(1);

/// Set the abort-timer used for default spawn [Config].
///
/// This is applied globally for processes spawned after setting this value.
pub fn set_default_abort_timer(timer: Duration) {
    DEFAULT_ABORT_TIMER_NANOS.store(timer.subsec_nanos(), Ordering::Release);
    DEFAULT_ABORT_TIMER_SECS.store(timer.as_secs(), Ordering::Release);
}

/// Get the current default abort-timer used for the default [Config].
pub fn get_default_abort_timer() -> Duration {
    Duration::new(
        DEFAULT_ABORT_TIMER_SECS.load(Ordering::Acquire),
        DEFAULT_ABORT_TIMER_NANOS.load(Ordering::Acquire),
    )
}

//------------------------------------------------------------------------------------------------
//  spawn
//------------------------------------------------------------------------------------------------

pub fn spawn<I, E, Fun, Fut>(function: Fun) -> (Child<E, I>, Address<I>)
where
    Fun: FnOnce(I) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: InboxType,
    I::Config: Default,
    E: Send + 'static,
{
    spawn_with(Default::default(), Default::default(), function)
}

pub fn spawn_with<I, E, Fun, Fut>(
    link: Link,
    config: I::Config,
    function: Fun,
) -> (Child<E, I>, Address<I>)
where
    Fun: FnOnce(I) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: InboxType,
    E: Send + 'static,
{
    let channel = I::setup_channel(config, 1, ActorId::generate());
    let inbox = I::from_channel(channel.clone());
    let handle = tokio::task::spawn(async move { function(inbox).await });
    (
        Child::new(channel.clone(), handle, link),
        Address::from_channel(channel),
    )
}

pub fn spawn_group<I, E, Itm, Fun, Fut>(
    iter: impl ExactSizeIterator<Item = Itm>,
    function: Fun,
) -> (ChildGroup<E, I>, Address<I>)
where
    Fun: FnOnce(Itm, I) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: GroupInboxType,
    I::Config: Default,
    E: Send + 'static,
    Itm: Send + 'static,
{
    spawn_group_with(Default::default(), Default::default(), iter, function)
}

pub fn spawn_group_with<I, E, Itm, Fun, Fut>(
    link: Link,
    config: I::Config,
    iter: impl ExactSizeIterator<Item = Itm>,
    function: Fun,
) -> (ChildGroup<E, I>, Address<I>)
where
    Fun: FnOnce(Itm, I) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: GroupInboxType,
    E: Send + 'static,
    Itm: Send + 'static,
{
    let channel = I::setup_multi_channel(config, iter.len(), 1, ActorId::generate());
    let handles = iter
        .map(|i| {
            let fun = function.clone();
            let inbox = I::from_channel(channel.clone());
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
    #[error("This channel does not allow spawning of multiple processes")]
    NotPermitted(T),
}

impl<T> std::fmt::Debug for TrySpawnError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exited(_) => f.debug_tuple("Exited").finish(),
            Self::IncorrectType(_) => f.debug_tuple("IncorrectType").finish(),
            Self::NotPermitted(_) => f.debug_tuple("NotPermitted").finish(),
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

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn default_abort_timer() {
        assert_eq!(get_default_abort_timer(), Duration::from_secs(1));
        set_default_abort_timer(Duration::from_secs(2));
        assert_eq!(get_default_abort_timer(), Duration::from_secs(2));
    }
}
