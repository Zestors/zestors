/*!
# Building supervision trees
Supervision trees can be built by spawning tasks and monitoring them using their children.
A [`Child`] is a unique reference to an actor similar to a
[tokio JoinHandle](tokio::task::JoinHandle), but different in that it could refer to multiple
processes (see [`ChildGroup`]) instead of just one. By default, when a child is dropped the
processes of the actor are shut down as well.

Monitoring is slightly different for single children and for child-groups:
- A [`Child<E, _, Single>`] can be monitored by awaiting it. When the actor exits normally, the
`E` (exit-value) of the process is returned, if it exits due to a panic or abort an [ExitError] is
returned instead.
- A [`Child<E, _, Group>`] can be monitored by [streaming](futures::Stream) it. The items returned
from the stream are the same as what is returned by awaiting a single child.

# Address
When an actor is spawned, it also returns an [`Address`] acting similar to a [`Child`].
There are some big differences between an address and a child:
- An [`Address`] is clonable, while a [`Child`] is unique to a given actor.
- When an [`Address`] is dropped, the actor is never shut down.
- Monitoring an [`Address`] always returns `()`, regardless of what the processes have exited with.
- An [`Address`] can not be used to abort the actor.

# Stopping an actor
An actor can be stopped in three different ways:

- __Halting:__ An actor can be halted using its [`Child`], [`Address`] or anything else that
implements [`ActorRef`]. Once the actor is aborted, all processes should clean up their state
and then exit gracefully. When the actor is halted, its [`Inbox`] is also automatically closed,
and no more messages can be sent to the inbox.

- __Aborting:__ An actor can be aborted using [`Child::abort`]. Aborting will forcefully interrupt
the process at its first `.await` point, and not allow it to clean up it's state before exiting.
([see tokio abort](tokio::task::JoinHandle::abort))

- __Shutting down:__ An actor can be shut down using its [`Child`]. This will first attempt to
halt the actor until a certain amout of time has passed, if the actor has not exited by that
point it is aborted instead.

# Actor references
Any type which implements [`ActorRef`] automatically implements [`ActorRefExt`] and [`ActorRefExtDyn`]
if it is a dynamic channel. This trait is implemented for [`Child`], [`Address`] and any inbox-types
like [`Inbox`] and [`Halter`]. These traits contain most methods for interacting with the actor.

# Actor state
The state of an actor can be queried from its [`Child`], [`Address`] or anything else that
implements [`ActorRef`] with four different methods:
- `has_exited`: Returns true if all [inboxes](InboxType) have been dropped.
- `is_closed`: Returns true if the [inbox](InboxType) has been closed.
- `is_aborted`: Returns true if the actor has been aborted. (Only available on children)
- `is_finished`: Returns true if all underlying tasks have finished. (Only available on children)

# Child or ChildPool
When specifying a [`Child<_, _, C>`], the argument `C` is it's [`ChildKind`]. This specifies
whether the child is a single child or a child-group. This type can be left-out with the default
being [`Single`]:
- `Child<_, _>` = `Child<_, _, Single>`
- `ChildPool<_, _>` = `Child<_, _, Group>`

Any child can be converted into a child-group using [`Child::into_group`].

| __<--__ [`messaging`] | [`actor_type`] __-->__ |
|---|---|

# Example
```
# tokio_test::block_on(main());
use zestors::{*, inboxes::inbox::RecvError, monitoring::ExitError};
use std::time::Duration;
use futures::stream::StreamExt;

// Let's create a basic process which we can monitor afterwards.
async fn spawned_process(mut inbox: Inbox<()>) -> &'static str {
    loop {
        match inbox.recv().await {
            Err(RecvError::ClosedAndEmpty) => {
                // The inbox was closed and is now empty
                break "Closed and empty"
            }
            Err(RecvError::Halted) => {
                // We have been halted
                break "Halt properly handled"
            }
            Ok(()) => {
                // We successfully received a message
                panic!("Unhandled message :(")
            }
        }

    }
}

async fn main() {
    // Let's spawn a process and halt it.
    let (child, address) = spawn( spawned_process);
    child.halt();
    assert!(matches!(child.await, Ok("Halt properly handled")));
    assert_eq!(address.await, ());

    // Properly shutting down an actor can be done as follows.
    // Here we give the actor 1 second to halt before we abort it.
    let (mut child, address) = spawn( spawned_process);
    child.shutdown(Duration::from_secs(1));
    assert!(matches!(child.await, Ok("Halt properly handled")));
    assert_eq!(address.await, ());

    // We can also directly abort it. (Not advised though :))
    let (mut child, address) = spawn( spawned_process);
    child.abort();
    assert!(matches!(child.await, Err(ExitError::Abort)));
    assert_eq!(address.await, ());

    // If we close the inbox the child also exits.
    let (mut child, address) = spawn( spawned_process);
    child.close();
    assert!(matches!(child.await, Ok("Closed and empty")));
    assert_eq!(address.await, ());

    // And if the actor panics then we can see it exits as well.
    let (mut child, address) = spawn( spawned_process);
    child.send(()).await.unwrap();
    assert!(matches!(child.await, Err(ExitError::Panic(_))));
    assert_eq!(address.await, ());

    // It is also possible to see that an actor gets shut down when the child is dropped.
    // This is the same as calling `shutdown` on the child.
    let (child, address) = spawn( spawned_process);
    drop(child);
    assert_eq!(address.await, ());

    // Let's also try spawning a child-group and monitoring that
    let (mut child_group, address) = spawn_group(0..10, |_, inbox| async move {
        spawned_process(inbox).await
    });
    address.halt();
    child_group
        .for_each(|process_exit| async move { // (trait StreamExt)
            assert!(matches!(process_exit, Ok("Halt properly handled")));
        })
        .await;
    assert_eq!(address.await, ());

    // And finally shutting down a child-group
    let (mut child_group, address) = spawn_group(0..10, |_, inbox| async move {
        spawned_process(inbox).await
    });
    child_group
        .shutdown(Duration::from_secs(1))
        .for_each(|process_exit| async move { // (trait StreamExt)
            assert!(matches!(process_exit, Ok("Halt properly handled")));
        })
        .await;
    assert_eq!(address.await, ());
}
```
*/

#[allow(unused)]
use crate::{*, all::*};

mod address;
mod child;
mod child_kind;
mod shutdown;
mod actor_ref;

pub use actor_ref::*;
pub use address::*;
pub use child::*;
pub use child_kind::*;
pub use shutdown::*;

pub use zestors_core::monitoring::*;
