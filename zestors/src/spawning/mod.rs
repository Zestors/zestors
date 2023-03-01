/*!
# Spawn functions
When spawning an actor, there are a couple of options to choose from:
- [`spawn(FnOnce)`](spawn) - This is the simplest way to spawn an actor and uses a default [`Link`] and
[`InboxType::Config`].
- [`spawn_with(link, cfg, FnOnce)`](spawn_with) - Same as `spawn`, but allows for a custom link and config.
- [`spawn_many(iter, FnOnce)`](spawn_many) - Same as `spawn`, but instead spawns many processes using
the iterator as the first argument to the function.
- [`spawn_many_with(iter, link, cfg, FnOnce)`](spawn_many_with) - Same as `spawn_many`, but allows for a
custom link and config.

It is also possible to spawn more processes onto an actor that is already running with
[`ChildPool::spawn_onto`] and [`ChildPool::try_spawn_onto`].

# Link
Every actor is spawned with a [`Link`] that indicates whether the actor is attached
or detached. By default a [`Link`] is attached with an abort-timer of 1 second; this means that when the
[`Child`] is dropped, the actor has 1 second to halt before it is aborted. If the link is detached, then
the child can be dropped without halting or aborting the actor.

# Inbox configuration
Actors can be spawned with any [`InboxType`], where the [`InboxType::Config`] specifies the configuration-options
for that inbox. The config for an [`Inbox`] is [`Capacity`] and for a [`Halter`] it is `()`.

The [`Capacity`] of an inbox can be one of three options:
- [`Capacity::Bounded(size)`](`Capacity::Bounded) --> An inbox that doesn't accept new messages after the
given size has been reached.
- [`Capacity::Unbounded`](Capacity::Unbounded) --> An inbox that grows in size infinitely when new messages
 are received.
- [`Capacity::BackPressure(BackPressure)`](Capacity::BackPressure) (default) --> An unbounded inbox with
a [`BackPressure`] mechanic. An overflow of messages is handled by increasing the delay for sending a message.

| __<--__ [`actor_type`](crate::actor_type) | [`handler`](crate::handler) __-->__ |
|---|---|

# Example
```
    use std::time::Duration;
    use zestors::{messaging::RecvError, prelude::*};
    use tokio::time::sleep;

    // Let's start by writing an actor that receives messages until it is halted ..
    async fn inbox_actor(mut inbox: Inbox<()>) {
        loop {
            if let Err(RecvError::Halted) = inbox.recv().await {
                break ();
            }
        }
    }

    //  .. and an actor that simply waits until it is halted.
    async fn halter_actor(halter: Halter) {
        halter.await
    }

    # tokio_test::block_on(main());
    async fn main() {
        // The `Halter` takes `()` as its config ..
        let _ = spawn_with(Link::default(), (), halter_actor);
        // .. while an `Inbox` takes a `Capacity`.
        let _ = spawn_with(Link::default(), Capacity::default(), inbox_actor);

        // Let's spawn an actor with default parameters ..
        let (child, address) = spawn(inbox_actor);
        drop(child);
        sleep(Duration::from_millis(10)).await;
        // .. and when the child is dropped, the actor is aborted.
        assert!(address.has_exited());

        // But if we spawn a detached child ..
        let (child, address) = spawn_with(Link::Detached, Capacity::default(), inbox_actor);
        drop(child);
        sleep(Duration::from_millis(10)).await;
        // .. the actor does not get halted.
        assert!(!address.has_exited());

        // We can also spawn a multi-process actor with the `Inbox` ..
        let (mut child_pool, address) = spawn_many(0..10, |i, inbox| async move {
            println!("Spawning process nr {i}");
            inbox_actor(inbox).await
        });
        // .. but that does not compile with a `Halter`. (use the `MultiHalter` instead)
        // let _ = spawn_many(0..10, |i, halter| async move {
        //     println!("Spawning process nr {i}");
        //     halter_actor(halter).await
        // });

        // We can now spawn additional processes onto the actor ..
        child_pool.spawn_onto(
            |_inbox: Inbox<()>| async move { () }
        ).unwrap();

        // .. and that is also possible with a dynamic child-pool ..
        let mut child_pool = child_pool.into_dyn();
        child_pool.try_spawn_onto(
            |_inbox: Inbox<()>| async move { () }
        ).unwrap();

        // .. but fails if given the wrong inbox-type.
        child_pool.try_spawn_onto(
            |_inbox: MultiHalter| async move { unreachable!() }
        ).unwrap_err();
    }
```
 */

mod errors;
mod functions;
mod capacity;
mod link;
pub use {capacity::*, errors::*, functions::*, link::*};
#[allow(unused)]
use crate::all::*;

