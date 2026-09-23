# Messages and interfaces

## Messages

A message is any type that implements `Message`, which is almost always derived.
The derive decides one thing: whether the message expects a reply.

```rust
use zestors::interface::Message;

// Fire-and-forget: nothing comes back to the sender.
#[derive(Message, Debug)]
struct Increment;

// Request/reply: calling this message returns a `u32`.
#[derive(Message, Debug)]
#[msg(reply = u32)]
struct GetCount;
```

Under the hood, `Message` has two associated types:

- `Output` is what the sender gets back: `u32` for `GetCount`, `()` for
  `Increment`.
- `Kind` is `Call` for a message with a reply and `Cast` for one without. It
  decides the pair of types that carry the reply. A `Call` message travels with
  a `Request<T>`, which the receiver answers, and the sender keeps the matching
  `Reply<T>` to wait on. A `Cast` message uses `()` for both.

Messages can be generic, and any `Send + 'static` type can be one.

## Interfaces

An actor accepts a set of messages, described by an `Interface`: an enum with
one variant per message, each wrapping the message in an `Envelope`.

```rust
use zestors::interface::{Envelope, Interface, Message};
# #[derive(Message, Debug)]
# struct Increment;
# #[derive(Message, Debug)]
# #[msg(reply = u32)]
# struct GetCount;

#[derive(Interface, Debug)]
enum CounterInterface {
    Increment(Envelope<Increment>),
    GetCount(Envelope<GetCount>),
}
```

Every variant must be a tuple variant with exactly one `Envelope<M>` field, and
each message type may appear only once. The derive generates:

- conversions between each `Envelope<M>` and the enum, which is how a sent
  message becomes the value the actor receives;
- the interface's `Set`, the type-level list of messages it accepts, which
  [dynamic addresses](dynamic-addresses.md) are checked against;
- conversions to and from a type-erased `AnyEnvelope`, used when the sender
  doesn't know the actor's concrete interface.

The derive does not support generic enums.

An `Envelope<M>` holds the message as `envelope.msg` and the reply handle as
`envelope.req`. `envelope.reply(value)` answers a request. For a
fire-and-forget message `req` is `()`, and the envelope is simply dropped once
handled.

The derives also take attributes for crate paths and for
[distributed mode](distributed/remote-messages.md); they are listed in
[Derive attributes](reference/derive-attributes.md).
