use std::{assert_matches, time::Duration};
use zestors_runtime::{
    channel::{ActorOpsExt as _, ActorStatus, StrongAddress, ExitStatus, Pid},
    registry::Registry,
    spawn,
};

#[tokio::test]
async fn register_and_deregister_refcounts_basics() {
    let mut child = spawn(common::simplest_handler);
    let pid = child.pid().clone();

    assert!(Registry::local().get(&pid).is_some());
    assert_eq!(child.strong_count(), 2); // Child + Inbox
    assert_eq!(child.weak_count(), 2); // Registry + Spawn

    let address = child.address().clone();

    assert_eq!(child.strong_count(), 2);
    assert_eq!(child.weak_count(), 3);

    drop(address);

    assert_eq!(child.strong_count(), 2);
    assert_eq!(child.weak_count(), 2);

    child.watch_initialization().await.unwrap();

    println!("Signaling shutdown for child with pid: {:?}", child.pid());
    child.signal_shutdown();
    (&mut child).await.unwrap();

    assert!(Registry::local().get(&pid).is_some());
    assert_eq!(child.strong_count(), 1);
    assert_eq!(child.weak_count(), 1);

    let address = child.address().clone();
    drop(child);

    assert_eq!(address.status(), ActorStatus::Exited(ExitStatus::Normal));
    assert!(Registry::local().get(&pid).is_none());
    assert_eq!(address.strong_count(), 0);
    assert_eq!(address.weak_count(), 1);
}

#[tokio::test]
async fn register_and_deregister_custom() {
    let channel = StrongAddress::create(Pid::rand()).unwrap();
    assert!(Registry::local().get(channel.pid()).is_some());
    let child = channel.clone().spawn(common::simplest_handler).unwrap();

    assert_eq!(channel.strong_count(), 3); // Channel + Child + Inbox
    assert_eq!(channel.weak_count(), 2); // Registry + Spawn
    assert_matches!(channel.clone().spawn(common::simplest_handler), Err(_));

    child
        .shutdown_abort(Duration::from_millis(10))
        .await
        .unwrap();
}

mod common;
