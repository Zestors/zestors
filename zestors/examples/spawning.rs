use std::time::Duration;
use tokio::time::sleep;
use zestors::{messaging::RecvError, prelude::*};

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

#[tokio::main]
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
    let (mut child_pool, _address) = spawn_many(0..10, |i, inbox| async move {
        println!("Spawning process nr {i}");
        inbox_actor(inbox).await
    });
    // .. but that does not compile with a `Halter`. (use the `MultiHalter` instead)
    // let _ = spawn_many(0..10, |i, halter| async move {
    //     println!("Spawning process nr {i}");
    //     halter_actor(halter).await
    // });

    // We can now spawn additional processes onto the actor ..
    child_pool
        .spawn_onto(|_inbox: Inbox<()>| async move { () })
        .unwrap();

    // .. and that is also possible with a dynamic child-pool ..
    let mut child_pool = child_pool.into_dyn();
    child_pool
        .try_spawn_onto(|_inbox: Inbox<()>| async move { () })
        .unwrap();

    // .. but fails if given the wrong inbox-type.
    child_pool
        .try_spawn_onto(|_inbox: MultiHalter| async move { unreachable!() })
        .unwrap_err();
}
