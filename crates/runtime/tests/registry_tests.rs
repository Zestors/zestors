//! Tests for the process registry (src/registry/*): lookup, registration,
//! deregistration, typed/dynamic fetch, and `Name` itself.

use zestors_interface::{Envelope, Interface, Message};
use zestors_runtime::errors::DuplicateNameError;
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
async fn get_finds_a_registered_actor_and_none_for_an_unknown_name() {
    let name = common::test_name("registry_get");
    let child = spawn(name.clone(), common::simplest_handler).unwrap();

    let found = Registry::local().get(&name).expect("should be registered");
    assert_eq!(found.name(), &name);

    assert!(
        Registry::local()
            .get(&Name::new("definitely_not_registered"))
            .is_none()
    );

    child.signal_shutdown();
}

#[tokio::test]
async fn contains_reflects_registration_and_removal() {
    let name = common::test_name("registry_contains");
    assert!(!Registry::local().contains(&name));

    let child = spawn(name.clone(), common::simplest_handler).unwrap();
    assert!(Registry::local().contains(&name));

    child.signal_shutdown();
    child.watch_exit().await.unwrap();
    drop(child);

    assert!(!Registry::local().contains(&name));
}

#[tokio::test]
async fn get_typed_succeeds_for_the_right_interface_and_errors_otherwise() {
    let name = common::test_name("registry_get_typed");
    let child = spawn(name.clone(), |mut inbox: Inbox<PingInterface>| async move {
        while inbox.recv().await.is_some() {}
        Ok(())
    })
    .unwrap();

    let typed = Registry::local().get_typed::<PingInterface>(&name);
    assert!(typed.is_ok());

    let wrong = Registry::local().get_typed::<OtherInterface>(&name);
    assert!(matches!(wrong, Err(TypedRegistryError::TypeMismatch(p)) if p == name));

    let missing = Registry::local().get_typed::<PingInterface>(&Name::new("nope_not_here"));
    assert!(matches!(missing, Err(TypedRegistryError::NotFound(_))));

    child.signal_shutdown();
}

#[tokio::test]
async fn get_dyn_succeeds_for_an_accepted_subset_and_errors_otherwise() {
    let name = common::test_name("registry_get_dyn");
    let child = spawn(name.clone(), |mut inbox: Inbox<PingInterface>| async move {
        while inbox.recv().await.is_some() {}
        Ok(())
    })
    .unwrap();

    let accepted = Registry::local().get_dyn::<(Ping,)>(&name);
    assert!(accepted.is_ok());

    let rejected = Registry::local().get_dyn::<(u8,)>(&name);
    assert!(matches!(rejected, Err(TypedRegistryError::TypeMismatch(p)) if p == name));

    child.signal_shutdown();
}

// ============================================================================
// Registration lifecycle.
// ============================================================================

#[tokio::test]
async fn spawn_auto_registers_and_exit_plus_drop_auto_deregisters() {
    let name = common::test_name("auto_register_lifecycle");
    assert!(!Registry::local().contains(&name));

    let child = spawn(name.clone(), common::simplest_handler).unwrap();
    assert!(Registry::local().contains(&name));

    child.signal_shutdown();
    child.watch_exit().await.unwrap();
    // Still registered: `Child` itself is a strong reference.
    assert!(Registry::local().contains(&name));

    drop(child);
    // Dropping the last strong reference deregisters synchronously - no
    // polling or sleeping needed.
    assert!(!Registry::local().contains(&name));
}

#[tokio::test]
async fn duplicate_name_is_rejected_and_reports_the_name() {
    let name = common::test_name("duplicate_name");
    let child = spawn(name.clone(), common::simplest_handler).unwrap();

    let result = spawn(name.clone(), common::simplest_handler);
    assert!(matches!(result, Err(DuplicateNameError { name: ref p }) if *p == name));

    child.signal_shutdown();
}

#[tokio::test]
async fn a_rejected_duplicate_spawn_does_not_deregister_the_original() {
    // Regression test: `StrongAddress::create` used to construct a full
    // `StrongAddress` *before* registering it, so a rejected duplicate-name
    // attempt would still run that (never-actually-registered) address's
    // `Drop` impl, which unconditionally removed whatever was currently
    // registered under that name - deregistering the pre-existing, still
    // very much alive, actor as a side effect of the rejection.
    let name = common::test_name("dup_no_collateral_damage");
    let child = spawn(name.clone(), common::simplest_handler).unwrap();
    assert!(Registry::local().contains(&name));

    let duplicate = spawn(name.clone(), common::simplest_handler);
    assert!(duplicate.is_err());

    assert!(
        Registry::local().contains(&name),
        "the original actor must still be registered after a rejected duplicate spawn"
    );
    assert!(!child.is_dead());

    child.signal_shutdown();
    child.watch_exit().await.unwrap();
    drop(child);
    assert!(!Registry::local().contains(&name));
}

#[tokio::test]
async fn name_can_be_reused_once_the_previous_actor_is_fully_gone() {
    let name = common::test_name("name_reuse");
    let child1 = spawn(name.clone(), common::simplest_handler).unwrap();
    child1.signal_shutdown();
    child1.watch_exit().await.unwrap();
    drop(child1);

    let child2 = spawn(name.clone(), common::simplest_handler);
    assert!(child2.is_ok());

    child2.unwrap().signal_shutdown();
}

#[tokio::test]
async fn strong_address_create_also_registers() {
    let name = common::test_name("strong_address_registers");
    let strong: zestors_runtime::StrongAddress<()> =
        zestors_runtime::StrongAddress::create(name.clone()).unwrap();

    assert!(Registry::local().contains(&name));

    drop(strong);
    assert!(!Registry::local().contains(&name));
}

// ============================================================================
// fetch_addresses.
// ============================================================================

#[tokio::test]
async fn fetch_addresses_includes_every_currently_registered_actor() {
    let mut children = Vec::new();
    let mut names = Vec::new();
    for i in 0..5 {
        let name = common::test_name(&format!("fetch_all_{i}"));
        children.push(spawn(name.clone(), common::simplest_handler).unwrap());
        names.push(name);
    }

    let addresses = Registry::local().fetch_addresses().await;
    let fetched_names: std::collections::HashSet<_> =
        addresses.iter().map(|a| a.name().clone()).collect();
    for name in &names {
        assert!(
            fetched_names.contains(name),
            "fetch_addresses should include {name}"
        );
    }

    for child in children {
        child.signal_shutdown();
    }
}

// ============================================================================
// Name.
// ============================================================================

#[test]
fn rand_names_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for _ in 0..500 {
        assert!(seen.insert(Name::rand()), "Name::rand produced a duplicate");
    }
}

#[test]
fn name_conversions_round_trip_through_string() {
    let name = Name::new("some-name");
    let s: String = (&name).into();
    assert_eq!(s, "some-name");
    assert_eq!(Name::new(s), name);
}

#[tokio::test]
async fn a_names_address_mirrors_the_registry() {
    let name = common::test_name("name_helper_methods");
    let child = spawn(name.clone(), |mut inbox: Inbox<PingInterface>| async move {
        while inbox.recv().await.is_some() {}
        Ok(())
    })
    .unwrap();

    assert!(name.address().is_some());
    assert!(Registry::local().get_typed::<PingInterface>(&name).is_ok());
    assert!(
        Registry::local()
            .get_typed::<OtherInterface>(&name)
            .is_err()
    );

    child.signal_shutdown();
    child.watch_exit().await.unwrap();
    drop(child);

    assert!(name.address().is_none());
}

// #[tokio::test]
// async fn name_current_and_parent_reflect_the_spawn_tree() {
//     let (tx, rx) = tokio::sync::oneshot::channel();

//     let parent = spawn_rand(move |mut inbox: Inbox<()>| async move {
//         let parent_name = inbox.name().clone();
//         assert_eq!(Name::current(), Some(parent_name.clone()));
//         assert_eq!(Name::parent(), None, "a top-level spawn has no parent");

//         // A panic here is caught by the runtime and turned into an
//         // `ExitStatus::Panicked` on this child, rather than unwinding into
//         // the test - so we report success/failure back through the oneshot
//         // instead of relying on an in-task panic to surface directly.
//         let child = spawn_rand(move |mut child_inbox: Inbox<()>| {
//             let parent_name = parent_name.clone();
//             async move {
//                 assert_eq!(Name::current(), Some(child_inbox.name().clone()));
//                 assert_eq!(Name::parent(), Some(parent_name));
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
//         "the child's Name::current/Name::parent assertions must have held"
//     );

//     parent.signal_shutdown();
//     assert!(
//         parent.watch_exit().await.is_ok(),
//         "the parent's own assertions must have held too"
//     );
// }
