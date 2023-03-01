/*!
# Overview
An actor reference is anything that implements [`ActorRef`]; examples include [`Child`], 
[`Address`], [`Inbox`] and [`Halter`]. An actor-reference can be used to interact with the actor: 
You can for example send messages, close the inbox or halt it using the [`ActorRefExt`] trait.

When an actor is spawned, it returns tuple of a [`Child`] and [`Address`]. The child is a unique
reference to the actor similar to a [tokio JoinHandle](tokio::task::JoinHandle). By default, when
the child is dropped the actor is shut down, therefore it can be used to build supervision-trees. If
the actor is [detached](Child::detach) then the actor won't be shut-down upon dropping the child. 
The address is a cloneable reference to the actor that can be shared with other processes to allow 
them to communicate.

# Monitoring
An actor can be monitored using it's [`Child`] or [`Address`] by awaiting them. When the actor exits,
it will notify the child and address and they return a value; a `Child<E, _>` returns a
[`Result<E, ExitError>`](ExitError), while an `Address<_>` returns `()`. When monitoring a 
[`ChildPool<E, _>`], instead of returning a single `Result<E, ExitError>`, a [`Stream`](futures::Stream) 
of these values is returned.

# Stopping an actor
An actor can be stopped in three different ways:

- __Halting:__ An actor can be halted using its [`Child`], [`Address`] or anything else that
implements [`ActorRef`]. When the actor is halted it should clean up it's state and then exit gracefully.

- __Aborting:__ An actor can be aborted with [`Child::abort`]. Aborting will forcefully interrupt
the process at its first `.await` point, and does not allow it to clean up it's state before exiting.
([see tokio abort](tokio::task::JoinHandle::abort))

- __Shutting down:__ An actor can be shut down using its [`Child::shutdown`]. This will first attempt to
halt the actor until a certain amout of time has passed, and if the actor has not exited by that
point it is aborted instead. This is the advised way of shutting down an actor in most cases.

# Actor state
The state of an actor can be queried from any [`ActorRef`] with four different methods:
- `has_exited`: Returns true if all [inboxes](`InboxType`) have been dropped.
- `is_closed`: Returns true if the channel has been closed and does not accept new messages.
- `is_aborted`: Returns true if the actor has been aborted. (Only available on a [`Child`])
- `is_finished`: Returns true if all underlying tasks have finished. (Only available on a [`Child`])

# Child or ChildPool
When specifying a [`Child<_, _, C>`], the argument `C` is the [`ChildType`]. This specifies
whether the child is a single [`Child`] or [`ChildPool`]. A child can be converted into a child-pool 
using [`Child::into_pool`].
- `Child<_, _>` = `Child<_, _, SingleProcess>`
- `ChildPool<_, _>` = `Child<_, _, MultiProcess>`

| __<--__ [`messaging`] | [`actor_type`] __-->__ |
|---|---|

# Example
```
# tokio_test::block_on(main());
use zestors::{prelude::*, messaging::RecvError, actor_reference::ExitError};
use std::time::Duration;
use futures::stream::StreamExt;

// Let's start by creating a simple event-loop for our actor.
async fn my_actor(mut inbox: Inbox<()>) -> &'static str {
    // This actor receives a single event only.
    match inbox.recv().await {
        Err(RecvError::ClosedAndEmpty) => {
            "Closed and empty"
        }
        Err(RecvError::Halted) => {
            "Halt properly handled"
        }
        Ok(_msg) => {
            panic!(r"\('o')/ This actor panics upon receiving a message!")
        }
    }
}

// We will now spawn the actor a bunch of times, but do different things with it to
// show of different functionalities.
async fn main() {
    // Halting an actor:
    let (child, address) = spawn(my_actor);
    child.halt();
    assert!(matches!(child.await, Ok("Halt properly handled")));
    assert_eq!(address.await, ());

    // Shutting down an actor:
    let (mut child, address) = spawn(my_actor);
    child.shutdown();
    assert!(matches!(child.await, Ok("Halt properly handled")));
    assert_eq!(address.await, ());

    // Aborting an actor:
    let (mut child, address) = spawn(my_actor);
    child.abort();
    assert!(matches!(child.await, Err(ExitError::Abort)));
    assert_eq!(address.await, ());

    // Closing the inbox:
    let (mut child, address) = spawn(my_actor);
    child.close();
    assert!(matches!(child.await, Ok("Closed and empty")));
    assert_eq!(address.await, ());

    // Making it panic by sending a message:
    let (mut child, address) = spawn(my_actor);
    child.send(()).await.unwrap();
    assert!(matches!(child.await, Err(ExitError::Panic(_))));
    assert_eq!(address.await, ());

    // Dropping the child:
    let (child, address) = spawn(my_actor);
    drop(child);
    assert_eq!(address.await, ());

    // Halting a child-pool:
    let (mut child_pool, address) = spawn_many(0..10, |_, inbox| async move {
        my_actor(inbox).await
    });
    address.halt();
    child_pool
        .for_each(|process_exit| async move {
            assert!(matches!(process_exit, Ok("Halt properly handled")));
        })
        .await;
    assert_eq!(address.await, ());

    // Shutting down a child-pool
    let (mut child_pool, address) = spawn_many(0..10, |_, inbox| async move {
        my_actor(inbox).await
    });
    child_pool
        .shutdown()
        .for_each(|process_exit| async move {
            assert!(matches!(process_exit, Ok("Halt properly handled")));
        })
        .await;
    assert_eq!(address.await, ());
}
```
*/

#[allow(unused)]
use crate::{all::*, *};

mod actor_ref;
mod address;
mod child;
mod child_type;
mod shutdown;
pub use actor_ref::*;
pub use address::*;
pub use child::*;
pub use child_type::*;
pub use shutdown::*;
