//! Tests for the process registry (src/registry/*): lookup, registration,
//! deregistration, typed/dynamic fetch, and `Pid` itself.

use zestors_interface::{Envelope, Interface, Message};
use zestors_runtime::errors::DuplicatePidError;
use zestors_runtime::prelude::*;
use zestors_runtime::{Registry, TypedRegistryError, spawn};

mod common;

#[derive(Message, Debug)]
#[zestors(interface_path = "zestors_interface")]
struct Ping;

#[derive(Interface, Debug)]
#[zestors(interface_path = "zestors_interface")]
enum PingInterface {
    Ping(Envelope<Ping>),
}

#[derive(Interface, Debug)]
#[zestors(interface_path = "zestors_interface")]
enum OtherInterface {
    Ping(Envelope<Ping>),
}

// ============================================================================
// Lookup.
// ============================================================================

#[tokio::test]
async fn get_finds_a_registered_actor_and_none_for_an_unknown_pid() {
    let pid = common::test_pid("registry_get");
    let child = spawn(pid.clone(), common::simplest_handler).unwrap();

    let found = Registry::local().get(&pid).expect("should be registered");
    assert_eq!(found.pid(), &pid);

    assert!(
        Registry::local()
            .get(&Pid::new("definitely_not_registered"))
            .is_none()
    );

    child.signal_shutdown();
}

#[tokio::test]
async fn contains_reflects_registration_and_removal() {
    let pid = common::test_pid("registry_contains");
    assert!(!Registry::local().contains(&pid));

    let child = spawn(pid.clone(), common::simplest_handler).unwrap();
    assert!(Registry::local().contains(&pid));

    child.signal_shutdown();
    child.watch_exit().await.unwrap();
    drop(child);

    assert!(!Registry::local().contains(&pid));
}

#[tokio::test]
async fn get_typed_succeeds_for_the_right_interface_and_errors_otherwise() {
    let pid = common::test_pid("registry_get_typed");
    let child = spawn(pid.clone(), |mut inbox: Inbox<PingInterface>| async move {
        while inbox.recv().await.is_some() {}
        Ok(())
    })
    .unwrap();

    let typed = Registry::local().get_typed::<PingInterface>(&pid);
    assert!(typed.is_ok());

    let wrong = Registry::local().get_typed::<OtherInterface>(&pid);
    assert!(matches!(wrong, Err(TypedRegistryError::TypeMismatch(p)) if p == pid));

    let missing = Registry::local().get_typed::<PingInterface>(&Pid::new("nope_not_here"));
    assert!(matches!(missing, Err(TypedRegistryError::NotFound(_))));

    child.signal_shutdown();
}

#[tokio::test]
async fn get_dyn_succeeds_for_an_accepted_subset_and_errors_otherwise() {
    let pid = common::test_pid("registry_get_dyn");
    let child = spawn(pid.clone(), |mut inbox: Inbox<PingInterface>| async move {
        while inbox.recv().await.is_some() {}
        Ok(())
    })
    .unwrap();

    let accepted = Registry::local().get_dyn::<(Ping,)>(&pid);
    assert!(accepted.is_ok());

    let rejected = Registry::local().get_dyn::<(u8,)>(&pid);
    assert!(matches!(rejected, Err(TypedRegistryError::TypeMismatch(p)) if p == pid));

    child.signal_shutdown();
}

// ============================================================================
// Registration lifecycle.
// ============================================================================

#[tokio::test]
async fn spawn_auto_registers_and_exit_plus_drop_auto_deregisters() {
    let pid = common::test_pid("auto_register_lifecycle");
    assert!(!Registry::local().contains(&pid));

    let child = spawn(pid.clone(), common::simplest_handler).unwrap();
    assert!(Registry::local().contains(&pid));

    child.signal_shutdown();
    child.watch_exit().await.unwrap();
    // Still registered: `Child` itself is a strong reference.
    assert!(Registry::local().contains(&pid));

    drop(child);
    // Dropping the last strong reference deregisters synchronously - no
    // polling or sleeping needed.
    assert!(!Registry::local().contains(&pid));
}

#[tokio::test]
async fn duplicate_pid_is_rejected_and_reports_the_pid() {
    let pid = common::test_pid("duplicate_pid");
    let child = spawn(pid.clone(), common::simplest_handler).unwrap();

    let result = spawn(pid.clone(), common::simplest_handler);
    assert!(matches!(result, Err(DuplicatePidError { pid: ref p }) if *p == pid));

    child.signal_shutdown();
}

#[tokio::test]
async fn a_rejected_duplicate_spawn_does_not_deregister_the_original() {
    // Regression test: `StrongAddress::create` used to construct a full
    // `StrongAddress` *before* registering it, so a rejected duplicate-pid
    // attempt would still run that (never-actually-registered) address's
    // `Drop` impl, which unconditionally removed whatever was currently
    // registered under that pid - deregistering the pre-existing, still
    // very much alive, actor as a side effect of the rejection.
    let pid = common::test_pid("dup_no_collateral_damage");
    let child = spawn(pid.clone(), common::simplest_handler).unwrap();
    assert!(Registry::local().contains(&pid));

    let duplicate = spawn(pid.clone(), common::simplest_handler);
    assert!(duplicate.is_err());

    assert!(
        Registry::local().contains(&pid),
        "the original actor must still be registered after a rejected duplicate spawn"
    );
    assert!(!child.is_dead());

    child.signal_shutdown();
    child.watch_exit().await.unwrap();
    drop(child);
    assert!(!Registry::local().contains(&pid));
}

#[tokio::test]
async fn pid_can_be_reused_once_the_previous_actor_is_fully_gone() {
    let pid = common::test_pid("pid_reuse");
    let child1 = spawn(pid.clone(), common::simplest_handler).unwrap();
    child1.signal_shutdown();
    child1.watch_exit().await.unwrap();
    drop(child1);

    let child2 = spawn(pid.clone(), common::simplest_handler);
    assert!(child2.is_ok());

    child2.unwrap().signal_shutdown();
}

#[tokio::test]
async fn strong_address_create_also_registers() {
    let pid = common::test_pid("strong_address_registers");
    let strong: zestors_runtime::StrongAddress<()> =
        zestors_runtime::StrongAddress::create(pid.clone()).unwrap();

    assert!(Registry::local().contains(&pid));

    drop(strong);
    assert!(!Registry::local().contains(&pid));
}

// ============================================================================
// fetch_addresses.
// ============================================================================

#[tokio::test]
async fn fetch_addresses_includes_every_currently_registered_actor() {
    let mut children = Vec::new();
    let mut pids = Vec::new();
    for i in 0..5 {
        let pid = common::test_pid(&format!("fetch_all_{i}"));
        children.push(spawn(pid.clone(), common::simplest_handler).unwrap());
        pids.push(pid);
    }

    let addresses = Registry::local().fetch_addresses().await;
    let fetched_pids: std::collections::HashSet<_> =
        addresses.iter().map(|a| a.pid().clone()).collect();
    for pid in &pids {
        assert!(
            fetched_pids.contains(pid),
            "fetch_addresses should include {pid}"
        );
    }

    for child in children {
        child.signal_shutdown();
    }
}

// ============================================================================
// Pid.
// ============================================================================

#[test]
fn rand_pids_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for _ in 0..500 {
        assert!(seen.insert(Pid::rand()), "Pid::rand produced a duplicate");
    }
}

#[test]
fn pid_conversions_round_trip_through_string() {
    let pid = Pid::new("some-name");
    let s: String = (&pid).into();
    assert_eq!(s, "some-name");
    assert_eq!(Pid::new(s), pid);
}

#[tokio::test]
async fn pid_address_and_typed_address_mirror_the_registry() {
    let pid = common::test_pid("pid_helper_methods");
    let child = spawn(pid.clone(), |mut inbox: Inbox<PingInterface>| async move {
        while inbox.recv().await.is_some() {}
        Ok(())
    })
    .unwrap();

    assert!(pid.address().is_some());
    assert!(pid.typed_address::<PingInterface>().is_ok());
    assert!(pid.typed_address::<OtherInterface>().is_err());

    child.signal_shutdown();
    child.watch_exit().await.unwrap();
    drop(child);

    assert!(pid.address().is_none());
}

// #[tokio::test]
// async fn pid_current_and_parent_reflect_the_spawn_tree() {
//     let (tx, rx) = tokio::sync::oneshot::channel();

//     let parent = spawn_rand(move |mut inbox: Inbox<()>| async move {
//         let parent_pid = inbox.pid().clone();
//         assert_eq!(Pid::current(), Some(parent_pid.clone()));
//         assert_eq!(Pid::parent(), None, "a top-level spawn has no parent");

//         // A panic here is caught by the runtime and turned into an
//         // `ExitStatus::Panicked` on this child, rather than unwinding into
//         // the test - so we report success/failure back through the oneshot
//         // instead of relying on an in-task panic to surface directly.
//         let child = spawn_rand(move |mut child_inbox: Inbox<()>| {
//             let parent_pid = parent_pid.clone();
//             async move {
//                 assert_eq!(Pid::current(), Some(child_inbox.pid().clone()));
//                 assert_eq!(Pid::parent(), Some(parent_pid));
//                 while child_inbox.recv().await.is_some() {}
//                 Ok(())
//             }
//         });
//         child.watch_init().await.unwrap();
//         child.signal_shutdown();
//         let child_exit = child.watch_exit().await;
//         let _ = tx.send(child_exit.is_ok());

//         while inbox.recv().await.is_some() {}
//         Ok(())
//     });

//     assert!(
//         rx.await.unwrap(),
//         "the child's Pid::current/Pid::parent assertions must have held"
//     );

//     parent.signal_shutdown();
//     assert!(
//         parent.watch_exit().await.is_ok(),
//         "the parent's own assertions must have held too"
//     );
// }
