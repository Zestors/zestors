use futures::stream::StreamExt;
use zestors::{
    actor_reference::{ExitError, IntoAddress},
    messaging::RecvError,
    prelude::*,
    DynActor,
};

// Let's start by creating a simple event-loop for our actor.
async fn my_actor(mut inbox: Inbox<()>) -> &'static str {
    // This actor receives a single event only.
    match inbox.recv().await {
        Err(RecvError::ClosedAndEmpty) => "Closed and empty",
        Err(RecvError::Halted) => "Halt properly handled",
        Ok(_msg) => {
            panic!(r"\('o')/ This actor panics upon receiving a message!")
        }
    }
}

// We will now spawn the actor a bunch of times, but do different things with it to
// show of different functionalities.
#[tokio::main]
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
    let (child, address) = spawn(my_actor);
    child.close();
    assert!(matches!(child.await, Ok("Closed and empty")));
    assert_eq!(address.await, ());

    // Making it panic by sending a message:
    let (child, address) = spawn(my_actor);
    child.send(()).await.unwrap();
    assert!(matches!(child.await, Err(ExitError::Panic(_))));
    assert_eq!(address.await, ());

    // Dropping the child:
    let (child, address) = spawn(my_actor);
    drop(child);
    assert_eq!(address.await, ());

    // Halting a child-pool:
    let (child_pool, address) = spawn_many(0..10, |_, inbox| async move { my_actor(inbox).await });
    address.halt();
    child_pool
        .for_each(|process_exit| async move {
            assert!(matches!(process_exit, Ok("Halt properly handled")));
        })
        .await;
    assert_eq!(address.await, ());

    // Shutting down a child-pool
    let (mut child_pool, address) =
        spawn_many(0..10, |_, inbox| async move { my_actor(inbox).await });
    child_pool
        .shutdown()
        .for_each(|process_exit| async move {
            assert!(matches!(process_exit, Ok("Halt properly handled")));
        })
        .await;
    assert_eq!(address.await, ());

    // As a final example, we can use different bounds on a send-function:
    let (child, address) = spawn(my_actor);
    send_using_into(address.clone()).await;
    send_using_accepts(&address).await;
    child.await.unwrap();
}

// Here we use `IntoAddress` ..
async fn send_using_into(address: impl IntoAddress<DynActor!(())>) {
    address.into_address().send(()).await.unwrap();
}

// .. and here `Accepts` as the bound.
async fn send_using_accepts(address: &Address<impl Accepts<()>>) {
    address.send(()).await.unwrap();
}
