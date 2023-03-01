/*!
# Overview
The [`ActorType`] of a [`Child<_, A, _>`] or [`Address<A>`] specifies what type of actor it refers
to. Some of the things it specifies are the messages the actor can accept and the type of [`Channel`] used.

An [`ActorType`] can be specified in two different ways:
1) As the [`InboxType`] the actor is spawned with. Some examples of built-in inboxes are the [`Inbox<Protocol>`]
and the [`Halter`]. An actor can choose what inbox it is spawned with, and new inboxes can be created in third-party
crates.
2) As a [`DynActor<dyn T>`](struct@DynActor) where `T` is one of the [`dyn_types`], usually written with [`DynActor!`].
The actor is not specified by it's inbox, but instead by the messages it [`Accepts`].

# `DynActor!` macro
Writing [`DynActor`](struct@DynActor) types can become complicated, nstead of writing and remembering these types,
use the [`DynActor!`] macro to specify the [`ActorType`]:
- `DynActor!()` = `DynActor<dyn AcceptsNone>`
- `DynActor!(u32)` = `DynActor<dyn AcceptsOne<u32>>`
- `DynActor!(u32, u64)` = `DynActor<dyn AcceptsTwo<u32, u64>>`
- etc.

# Transforming actor-types
Any [`ActorRef`] that implements [`Transformable`] allows it's [`ActorType`] to be transformed into and
from a dynamic one. This allows references to different inbox-types to be transformed into ones of the same
actor-type.

[`Transformable`] is implemented for an [`Address<A>`] and [`Child<_, A, _>`], which allows them to be transformed
into an `Address<T>` and `Child<_, T, _>` as long as [`A: TransformInto<T>`](TransformInto).
Some examples with the default inbox-types:
- A [`Halter`] can be transformed into a `DynActor!()`.
- An [`Inbox<P>`] can be transformed into a `DynActor!(M1 .. Mn)` as long as `P` implements
[`FromPayload<M>`] for `M in [M1 .. Mn]`.
- A [`DynActor!(M1 .. Mn)`](DynActor!) can be transformed into a `DynActor!(T1 .. Tm)` as long
as [`T1 .. Tn`] ⊆ [`M1 .. Mm`].

For these examples transformation can be done with [`Transformable::transform_into`] with transformations
checked at compile-time. Transformations can also be checked at run-time (or not at all)
with [`Transformable::try_transform_into`] and [`Transformable::transform_unchecked_into`].

A [`DynActor`](struct@DynActor) can be downcast into the original [`InboxType`] with [`Transformable::downcast`].

| __<--__ [`actor_reference`](crate::actor_reference) | [`spawning`](crate::spawning) __-->__ |
|---|---|

# Example
```
# tokio_test::block_on(main());
use zestors::{prelude::*, protocol, DynActor};

// Simple protocol which accepts `u32` and `u64`
#[protocol]
enum MyProtocol {
    A(u32),
    B(u64),
}

async fn main() {
    // Let's spawn a process that will live forever
    let (child, address) = spawn(|_inbox: Inbox<MyProtocol>| {
        futures::future::pending::<()>();
    });

    // As we can see, the address and child are typed by `Inbox<MyProtocol>`.
    let child: Child<_, Inbox<MyProtocol>> = child;
    let address: Address<Inbox<MyProtocol>> = address;

    // Let's cast the address to a few different types:
    let _: Address = address.clone().transform_into();
    let _: Address<DynActor!(u32)> = address.clone().transform_into();
    let _: Address<DynActor!(u32, u64)> = address.clone().transform_into();
    // But this won't compile! The actor does not accept strings
    // let _: Address<DynActor!(String)> = address.clone().transform_into();

    // We can also transform a child:
    let dyn_child: Child<_, DynActor!(u32)> = child.transform_into();
    // And then downcast it to the original child:
    let child: Child<_, Inbox<MyProtocol>> = dyn_child.downcast().unwrap();

    // We can also keep transforming an address (or child) even if it is already dynamic.
    address
        .clone()
        .transform_into::<DynActor!(u64, u32)>()
        .transform_into::<DynActor!(u32)>()
        .transform_into::<DynActor!()>()
        .downcast::<Inbox<MyProtocol>>()
        .unwrap();

    // This is a transformation fails at runtime:
    address
        .clone()
        .transform_into::<DynActor!()>()
        .try_transform_into::<DynActor!(u32, String)>()
        .unwrap_err();

    // But if we use the unchecked transformation it doesn't complain
    let incorrect_address = address
        .clone()
        .transform_into::<DynActor!()>()
        .transform_unchecked_into::<DynActor!(u32, String)>();

    // If we sent it this message, it would panic
    // -> incorrect_address.send("error".to_string()).await;
    // Using the checked send results in an error instead
    incorrect_address.send_checked("error".to_string()).await.unwrap_err();
    // Though we can still send it correct messages
    incorrect_address.send_checked(10u32).await.unwrap();
}
```
*/

#[allow(unused)]
use crate::all::*;

mod actor_id;
mod actor_type;
mod channel;
mod dyn_actor;
mod errors;
mod halter;
mod inbox;
mod multi_halter;
pub use {
    actor_id::*, actor_type::*, channel::*, dyn_actor::*, errors::*, halter::*,
    inbox::*, multi_halter::*,
};

pub mod dyn_types;
