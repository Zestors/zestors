# Dynamic addresses

Every actor reference has a *context*: what the reference knows the actor
accepts. It is the type parameter in `Address<C>`, `Child<E, C>`,
`StrongAddress<C>`, and so on. There are two kinds:

- **An interface**, such as `Address<CounterInterface>`. The reference knows the
  actor's exact interface.
- **A `Dyn` set**, such as `Address<Dyn<(Increment, GetCount)>>`. The reference
  only knows that the actor accepts at least these messages, whatever its
  actual interface is.

`Dyn<()>`, or just `Dyn`, accepts nothing. It is what `Address` defaults to, and
what the `Registry` hands out when it doesn't know the type.

A `Dyn` address can send exactly the messages in its set, checked at compile
time, just like a typed one. What it hides is the concrete interface. Actors of
completely different types that share some messages therefore produce
addresses of the *same* type. That lets them be stored together, passed to the
same function, or handed out without revealing what else the actor does.

## Converting between contexts

The `IntoDyn` trait (in the prelude) converts a reference by value, and
`AsDyn` converts it by reference:

| By value (`IntoDyn`) | By reference (`AsDyn`) | Checked | Fails if |
| --- | --- | --- | --- |
| `into_dyn::<S>()` | `as_dyn::<S>()` | at compile time | — (it doesn't compile unless `S` is a subset of what the context accepts) |
| `into_dyn_checked::<S>()` | `as_dyn_checked::<S>()` | at runtime | the actor doesn't accept every message in `S` |
| `downcast::<I>()` | `downcast_ref::<I>()` | at runtime | the actor's interface isn't exactly `I` |

The runtime-checked ones return the original reference on failure (`Err(self)`
or `None`), so nothing is lost. `into_context_unchecked` skips the check
altogether. A wrong context doesn't cause undefined behaviour, but sending a
message the actor doesn't accept then panics.

## Example: many kinds of actor, one list

Two unrelated actors both answer `GetHealth`. Narrowed to
`Dyn<(GetHealth,)>`, they fit in one `Vec`:

```rust
use zestors::interface::{Envelope, Interface, Message};
use zestors::prelude::*;
use zestors::runtime::{Dyn, spawn_rand};
use zestors::supervision::messages::{GetHealth, Health};

#[derive(Message, Debug)]
struct Ping;

#[derive(Interface, Debug)]
enum WorkerInterface {
    Ping(Envelope<Ping>),
    Health(Envelope<GetHealth>),
}

#[derive(Message, Debug)]
#[msg(reply = "Option<String>")]
struct Get(String);

#[derive(Interface, Debug)]
enum CacheInterface {
    Get(Envelope<Get>),
    Health(Envelope<GetHealth>),
}

# #[tokio::main]
# async fn main() {
let worker = spawn_rand(|mut inbox: Inbox<WorkerInterface>| async move {
    while let Some(msg) = inbox.recv().await {
        if let WorkerInterface::Health(env) = msg {
            let _ = env.reply(Health::healthy());
        }
    }
    Ok(())
});
let cache = spawn_rand(|mut inbox: Inbox<CacheInterface>| async move {
    while let Some(msg) = inbox.recv().await {
        match msg {
            CacheInterface::Get(env) => { let _ = env.reply(None); }
            CacheInterface::Health(env) => { let _ = env.reply(Health::degraded()); }
        }
    }
    Ok(())
});

// Checked at compile time: both interfaces contain `GetHealth`.
let monitored: Vec<Address<Dyn<(GetHealth,)>>> = vec![
    worker.address().clone().into_dyn(),
    cache.address().clone().into_dyn(),
];

for address in &monitored {
    let health = address.call(GetHealth).await.unwrap();
    println!("{}: {health}", address.name());
}

// The concrete type can be recovered, checked at runtime.
let back = monitored[1].clone().downcast::<CacheInterface>().unwrap();
assert_eq!(back.call(Get("key".into())).await.unwrap(), None);
assert!(monitored[0].clone().downcast::<CacheInterface>().is_err());
# worker.signal_shutdown();
# cache.signal_shutdown();
# }
```

## Without a typed reference at all

Sometimes all you have is an untyped `Address` (an `Address<Dyn>`), for example
from `Name::address()` or `Registry::get`. Then either:

- convert it with `into_dyn_checked`, or look it up with
  `Registry::local().get_dyn::<S>(&name)`, which does the same check; or
- send directly with `cast_dyn` / `call_dyn` (and their `try_`/`_with`
  variants) from `ActorOps`. These work on any reference, and return a
  `NotAccepted` error, with the message, if the actor doesn't take it.

## How the supervision tree uses this

`GetChildren` and `GetHealth` from `zestors::supervision::messages` are ordinary
messages. The supervision tree (`SupervisionTree`), the HTTP API server and the
inspector explore a running system by looking actors up by name and sending
them `call_dyn(GetChildren)` and `call_dyn(GetHealth)`. They never need to know
an actor's type. So:

- a custom supervisor becomes part of the tree by accepting `GetChildren`;
- any actor can report its health by accepting `GetHealth`.

## Across the cluster

A `ClusterAddress` has the same kind of context. `Cluster::address::<I>()` makes
one for a whole interface, and `Cluster::address_dyn::<(A, B)>()` for a set.
The set form also lets you reach the remote-capable part of an interface that
has local-only messages; see [Remote messages](distributed/remote-messages.md).

A `ClusterAddress` can't be converted between contexts yet. Pick the context
when you create the address.
