# Addressing and sending

## Getting a `ClusterAddress`

`Cluster` makes addresses:

| Method | For |
| --- | --- |
| `address::<I>(global_name)` | an actor, by its whole interface `I` |
| `address_dyn::<(A, B)>(global_name)` | an actor, by a set of messages it accepts |
| `local_address(address)` | an `Address` you already hold on this node |

For an actor on another node, `address` and `address_dyn` ask that node whether
an actor by that name is there, and whether it accepts the messages. They fail
with an `AddressError` if not. For a name on this node they look in the local
registry without touching the network.

```rust,ignore
{{#include ../../../crates/zestors/examples/remote.rs:call}}
```

A few things to know:

- **Wait for the node to be a member.** Addressing or sending to a node this
  node doesn't know yet fails with `CastFailure::NotAMember`. After starting,
  use `cluster.wait_for_members(n)` or `wait_until`.
- **An address follows the name, not the actor.** A remote `ClusterAddress` is
  resolved by name on the other node for every message. If the actor is
  restarted under the same name, for example by its supervisor, the address
  reaches the new one. If nothing holds the name any more, sends fail with
  `RemoteError::NoSuchActor`.
- **Every named actor is reachable.** Any registered actor on a node can be
  addressed by any other member, and sent any registered message it accepts.
  Access is controlled by cluster membership (the TLS certificates), not per
  actor.

## Sending

`ClusterAccepts` (in the prelude) is the counterpart of `Accepts`:

| Method | Waits for | Returns |
| --- | --- | --- |
| `cast(msg)` | room in the outgoing queue to that node | the receipt: `()`, or a `ClusterReply<T>` to `.wait()` on later |
| `try_cast(msg)` | nothing; fails with `CastFailure::Full` | the receipt |
| `call(msg)` | room, then the reply | the reply |

Each has a `*_with(msg, ClusterCallOptions)` variant.

Returning from `cast` means the message was queued for sending, not that it
arrived. Only a reply confirms that the actor got it.

## Timeouts

A call to an actor **on another node** waits at most:

1. the timeout in its `ClusterCallOptions`, if set; otherwise
2. the address's timeout, set with `ClusterAddress::with_timeout`; otherwise
3. the node's `ClusterConfig::call_timeout`, 30 seconds by default.

It then fails with `ClusterReplyError::Timeout`.

A call to an actor **on this node** only has a timeout if
`ClusterCallOptions::timeout` sets one. Otherwise it waits as long as a local
`call` would.

## Local and remote, one type

A `ClusterAddress` can point to either side. Code that holds one works the same
wherever the actor turns out to be, with these differences:

- a local actor gets the message itself, without encoding;
- a local actor's queue applies backpressure the usual way, while a remote node
  refuses messages when overloaded (see [Delivery and failure](delivery.md));
- the default timeout only applies to remote actors.

`is_local()`, `is_remote()` and `node()` tell you which it is.
`local_address()` returns the plain `Address` for a local actor, which you need
for messages that aren't remote.

## Wrapping a `ClusterAddress`

Implement `ClusterActorRef` for a type of your own that holds a
`ClusterAddress`, and it gets `ClusterAccepts` and `ClusterActorOps` too. This is
useful for a typed client struct around a remote service.
