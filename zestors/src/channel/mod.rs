/*!
# Specifying an actor-type
The [`ActorType`]  of a [`Child<_, A, _>`] or [`Address<A>`] specifies what type of actor it refers
to with the parameter `A`. Normally this type is the [`InboxType`] that the actor is spawned with,
i.e. an [`Inbox<P>`] or a [`Halter`], however this can also be defined dynamically. A dynamic
actor-type can be defined using [`Dyn<T>`] where `T` is [`dyn AcceptsNone`](AcceptsNone),
[`dyn AcceptsOne<_>`](AcceptsOne), [`dyn AcceptsTwo<_, _>`](AcceptsTwo) etc. By specifying an actor type
by the messages it accepts, we can cast addresses and inboxes of different types to the same type!

# Accepts! macro
Instead of writing and remembering long and complicated types, we can use the [`Accepts`] macro:
- `Accepts![]` = `Dyn<dyn AcceptsNone>`
- `Accepts![u32]` = `Dyn<dyn AcceptsOne<u32>>`
- `Accepts![u32, u64]` = `Dyn<dyn AcceptsTwo<u32, u64>>`
- etc.

# Transforming
Addresses and children of a given actor type `A` can be transformed into those of actor type `T`
as long as [`A: TransformInto<T>`](TransformInto). This is implemented in the following cases:
- If `A` is a [`Halter`] then this can be transformed into `Accepts![]`.
- If `A` is an [`Inbox<P>`] then if `P` implements [`ProtocolFrom<X>`] for `X = M1, ..., Mx`, the
actor-type can be transformed into `Accepts![M1, ..., Mx]`
- if `A` is an `Accepts[M1, ..., Mx]` then this can be transformed into `Accepts![T1, ..., Ty]` as long
as `T1, ..., Ty` is a subset of `M1, ..., Mx`.

A dynamic actor-types can also be transformed with `try_transform_into` or `transform_unchecked_into`.
The first method checks at runtime if the actor accepts all methods, while the second one transforms
without doing any checks. Sending messages to an actor which does not accept those messages will
panic.

# Downcasting
If an address is of a [`Dyn<_>`] type, it can be downcast back into the original [`ActorType`] `T` using
`downcast<T>`. This succeeds only if the actor-type is the same as the one the actor was spawned with.

# Sending messages
As long as the [`ActorType`] implements [`Accept<M>`], messages of type `M` can be sent to that
actor. Therefore messages can be sent to actors that are statically or dynamically typed.

| __<--__ [`monitoring`] | [`spawning`] __-->__ |
|---|---|

# Example
```
use zestors::*;
use futures::future::pending;

// Simple protocol which accepts `u32` and `u64`
#[protocol]
enum MyProtocol {
    A(u32),
    B(u64),
}

# tokio_test::block_on(main());
# #[allow(unused)]
async fn main() {
    // Here we spawn a process that never exits.
    let (child, address) = spawn( |_: Inbox<MyProtocol>| pending::<()>());

    // As we can see, the address and child are typed by `Inbox<MyProtocol>`.
    let child: Child<_, Inbox<MyProtocol>> = child;
    let address: Address<Inbox<MyProtocol>> = address;

    // Let's cast the address to a few different types:
    let _: Address = address.clone().transform_into();
    let _: Address<Accepts![u32]> = address.clone().transform_into();
    let _: Address<Accepts![u32, u64]> = address.clone().transform_into();
    // But this won't compile!
    // let _: Address<Accepts![String]> = address.clone().transform_into();

    // We can also transform a child:
    let dyn_child: Child<_, Accepts![u32]> = child.transform_into();
    // And then downcast it to the original child:
    let child: Child<_, Inbox<MyProtocol>> = dyn_child.downcast().unwrap();

    // We can also keep transforming an address (or child) even if it is already dynamic.
    address
        .clone()
        .transform_into::<Accepts![u64, u32]>()
        .transform_into::<Accepts![u32]>()
        .transform_into::<Accepts![]>()
        .downcast::<Inbox<MyProtocol>>()
        .unwrap();

    // This is a transformation that should fail:
    address
        .clone()
        .transform_into::<Accepts![]>()
        .try_transform_into::<Accepts![String]>()
        .unwrap_err();

    // But if we use the unchecked transformation it doesn't complain
    let incorrect_address = address
        .clone()
        .transform_into::<Accepts![]>()
        .transform_unchecked_into::<Accepts![u32, String]>();

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
mod errors;
mod accepts;
pub use {actor_id::*, actor_type::*, channel::*, errors::*, accepts::*};

pub mod dyn_actor_types;
pub mod halter;
pub mod inbox;
pub mod multi_halter;
