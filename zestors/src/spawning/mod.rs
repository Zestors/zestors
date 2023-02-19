/*!

# Spawn functions
When spawning an actor, there are a couple of options to choose from:
- [`spawn(f)`](spawn) - This is the simplest way to spawn an actor, and uses the default [`Link`] and
[`InboxType::Config`] to spawn function `f`.
- [`spawn_with(link, cfg, f)`](spawn_with) - Same as `spawn`, but allows for a
custom link and config.
- [`spawn_pool(iter, f)`](spawn_pool) - Same as `spawn`, but instead spawns many processes using
the iterator as the first argument to `f`.
- [`spawn_pool_with(iter, link, cfg, f)`](spawn_pool_with) - Same as `spawn_pool`, but allows for a
custom link and config.

The inbox's config can be different for any [`InboxType`]. A [`Halter`] has `()` as it's config,
while an [`Inbox`] uses a [`Capacity`].

## Spawning onto an actor that is running
It is also possible to spawn more processes onto an actor that is already running using
[`ChildPool::spawn_onto`] and [`ChildPool::try_spawn_onto`]. These methods will fail if the actor has
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

        // It is also possible to spawn a process-pool using an `Inbox`...
        let _ = spawn_pool(0..10, |i, inbox| async move {
            println!("Spawning process nr {i}");
            inbox_actor(inbox).await
        });
        // or a `Halter`...
        let _ = spawn_pool(0..10, |i, halter| async move {
            println!("Spawning process nr {i}");
            halter_actor(halter).await
        });
        // or either one using a custom config.
        let (mut child_pool, address) = spawn_pool_with(
            Link::Attached(Duration::from_millis(100)),
            Capacity::default(),
            0..10,
            |i, inbox| async move {
                println!("Spawning process nr {i}");
                inbox_actor(inbox).await
            },
        );

        // Since we have a child_pool, it is possible to spawn more processes onto
        // the actor:
        child_pool.spawn_onto(inbox_actor).unwrap();
        child_pool.spawn_onto(inbox_actor).unwrap();

        // This can even be done if the child_pool is dynamic:
        let mut child_pool = child_pool.into_dyn();
        child_pool.try_spawn_onto(inbox_actor).unwrap();
        child_pool.try_spawn_onto(inbox_actor).unwrap();

        // But fails if we spawn an actor with a different inbox-type:
        child_pool.try_spawn_onto(halter_actor).unwrap_err();
    }
```
 */

mod link;
mod errors;
pub use {link::*, errors::*};

use crate::*;
use futures::Future;

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

pub fn spawn_pool<I, E, Itm, Fun, Fut>(
    iter: impl ExactSizeIterator<Item = Itm>,
    function: Fun,
) -> (ChildPool<E, I>, Address<I>)
where
    Fun: FnOnce(Itm, I) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: MultiInboxType,
    I::Config: Default,
    E: Send + 'static,
    Itm: Send + 'static,
{
    spawn_pool_with(Default::default(), Default::default(), iter, function)
}

pub fn spawn_pool_with<I, E, Itm, Fun, Fut>(
    link: Link,
    config: I::Config,
    iter: impl ExactSizeIterator<Item = Itm>,
    function: Fun,
) -> (ChildPool<E, I>, Address<I>)
where
    Fun: FnOnce(Itm, I) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: MultiInboxType,
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
