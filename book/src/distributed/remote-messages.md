# Remote messages

A message can be sent to another node when it is a `RemoteMessage`. There is
nothing to implement: every message that

1. has a `StableId`,
2. can be encoded and decoded, and
3. has a reply type that can be encoded and decoded too

is one. With serde, that is one line of derives:

```rust
use serde::{Deserialize, Serialize};
use zestors::interface::Message;
use zestors::prelude::*;

#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = u64, id = "0e1c8a6e-6f7a-4b8e-9a53-3f2d1c0b9a87")]
struct Fibonacci(u32);
```

## Message ids

A node receives bytes, and needs to know which type to decode them as. Rust type
names aren't stable across builds, so each remote message carries a
`MessageId`: a UUID you choose once and never change. Leave the id out and the
compiler error suggests a freshly generated one to paste in.

- **Never change an id** once nodes running different builds may talk to each
  other: a node that doesn't know an id answers `RemoteError::UnknownMessage`.
- **Never reuse an id** for two types. Registering two types under one id
  panics when the node is built, because the other nodes would otherwise decode
  the bytes as whichever type won.
- Changing a message's *fields* changes its wire format. Nodes that disagree
  fail to decode it (`RemoteError::Decode`). Treat a message like any other
  wire format: add fields compatibly, or introduce a new message with a new id.

## Encoding

The wire format comes from the `Encode` and `Decode` traits. Every serde type
implements them, using [postcard](https://docs.rs/postcard). To use another
format, implement both traits by hand on a type that doesn't implement
`Serialize`; for a type that does, wrap it in a newtype first.

A message to an actor on the *same* node is never encoded: it is delivered as
the value itself. A type whose encoding is broken therefore works locally and
fails only once it crosses the network.

Messages are limited to 4 MiB once encoded; a larger one fails with
`CastFailure::TooLarge`.

## Registering what a node accepts

A node accepts a remote message only if it was registered when the node was
configured:

```rust,ignore
let config = ClusterConfig::new("node-b", backend)
    .register::<Fibonacci>()
    .register::<Greet>();
```

- Registering is only needed **on the receiving node**. Sending, and addressing
  an actor with `Cluster::address`, need nothing.
- Registration is fixed when the node is built. A running node can't start
  accepting a new message type, so there is never a moment where it serves
  requests it can't yet handle.
- A registered message can be sent to *any* actor on the node that accepts it.

### Registering everything at once

With the `auto-register` feature, `ClusterConfig::auto_register()` registers
every remote message in the binary: every non-generic type that derives
`StableId`. To leave one out, add `#[msg(no_auto_register)]`. Generic messages
always have to be registered by hand, once per concrete type.

`auto_register` silently skips a type that derives `StableId` but isn't a
`RemoteMessage`, for example because it doesn't derive `Serialize`. A sender then
gets `RemoteError::UnknownMessage` back. If that happens, check the derives.

## Interfaces with local-only messages

`Cluster::address::<I>()` addresses an actor by its whole interface, which
requires *every* message in `I` to be remote. An interface that mixes remote
and local-only messages doesn't compile there. Address the part that can cross
the network instead:

```rust,ignore
let remote_part = cluster
    .address_dyn::<(Fibonacci, Greet)>(GlobalName::new("worker", "node-b"))
    .await?;
```

The same is true for sending through a `ClusterAddress`: only remote messages
can be sent through one, even when the actor happens to be local. For a local
actor, `ClusterAddress::local_address()` gives back the ordinary `Address`, which
can send anything.

## Replies inside messages

A reply type is sent back automatically. When a message needs to *carry* a
reply channel, for example so the actor can hand it on to another actor, use a
`RemoteRequest` field; see [Replies inside messages](remote-requests.md).
