# Spawning and messaging actors

## Spawning

An actor's body is an async closure (or function) that takes an `Inbox` and
returns a `Result`. There are two ways to start one:

- `spawn(name, f)` registers the actor under a `Name` you choose. It fails if
  that name is taken.
- `spawn_rand(f)` generates a fresh, random name.

Both return a `Child`, the owning handle to the actor.

```rust
use zestors::prelude::*;
use zestors::runtime::{spawn, spawn_rand};

# #[tokio::main]
# async fn main() {
let child = spawn(Name::new("worker"), |mut inbox: Inbox<()>| async move {
    let mut seen = 0;
    while inbox.recv().await.is_some() {
        seen += 1;
    }
    Ok(seen)
})
.unwrap();

let other = spawn_rand(|mut inbox: Inbox<()>| async move {
    while inbox.recv().await.is_some() {}
    Ok(())
});
# child.signal_shutdown();
# other.signal_shutdown();
# }
```

`Inbox<()>` is the simplest interface: the actor accepts only `()`. Anything
more is a `#[derive(Interface)]` enum, as in [Messages and interfaces](messages.md).
The value the closure returns (`seen` above) is what awaiting the `Child` gives
back.

## References to an actor

| Type | Keeps the actor's name registered | Can be cloned | Notes |
| --- | --- | --- | --- |
| `Child` | yes | no | Owns the task: awaiting it gives the actor's return value. **Dropping it aborts the actor** unless `.detach()` was called. |
| `StrongAddress` | yes | yes | Can spawn a new task on the same name once the old one has exited. Supervisors use this to restart. |
| `Address` | no | yes | The everyday handle for sending messages. |
| `Inbox` | yes | no | Held by the actor itself, to receive. |

Messages are sent through the `Accepts` trait and everything else goes through
`ActorOps`; both are in the prelude. All four references implement them, so any
of them can send messages, send signals and read the status.

`Child` aborts on drop even when it is bound to `_` or simply goes out of scope.
Keep it, call `.detach()` on it, or turn it into a plain `JoinHandle` with
`.into_handle()`.

## Sending

| Method | Waits for | Returns |
| --- | --- | --- |
| `cast(msg)` | room in the queue (backpressure) | the receipt: `()` or a `Reply<T>` to await later |
| `try_cast(msg)` | nothing; fails if the queue is full | the receipt |
| `call(msg)` | room, then the reply | the reply itself |

Each has a `*_with(msg, CallOptions)` variant. `CallOptions` can ignore
backpressure, or deliver to an actor that is already exiting.

A message can only be sent to an actor whose interface contains it; anything
else is a compile error. To decide at runtime instead, use `cast_dyn` or
`call_dyn`, which return a `NotAccepted` error — see
[Dynamic addresses](dynamic-addresses.md).

A local `call` has no timeout: it waits until the actor replies or drops the
request. Wrap it in `tokio::time::timeout` if you need one.

## Finding actors by name

Every actor is registered in the process-wide `Registry` under its `Name`, for
as long as a strong reference (a `Child`, `StrongAddress` or `Inbox`) exists.
Code that only knows the name can look it up:

```rust
use zestors::prelude::*;
use zestors::runtime::{Registry, spawn};

# #[tokio::main]
# async fn main() {
let name = Name::new("counter");
let child = spawn(name.clone(), |mut inbox: Inbox<()>| async move {
    while inbox.recv().await.is_some() {}
    Ok(())
})
.unwrap();

// With the exact interface...
let typed: Address<()> = Registry::local().get_typed::<()>(&name).unwrap();
typed.cast(()).await.unwrap();

// ...or untyped, checking at runtime whether it accepts the message.
let untyped = name.address().unwrap();
untyped.cast_dyn(()).await.unwrap();
# child.signal_shutdown();
# }
```

The registry covers one process. To reach an actor on another node, see
[Addressing and sending](distributed/addressing.md).

## Receiving

The `Inbox` gives the actor what arrives, in a few ways:

- `recv()` returns the next message and skips signals. After a shutdown signal
  it drains the messages still queued, then returns `None`.
- `recv_event()` returns an `InboxEvent`, either a message or a `Signal`, so the
  actor can react to signals itself. It returns `None` once a shutdown has been
  received and the queue is empty.
- `recv_event_always()` is like `recv_event`, but keeps returning events after a
  shutdown signal, so the actor decides when to stop.
- `recv_signal()` returns only signals.
- `try_recv()` doesn't wait.

Because the actor owns its loop, it can `tokio::select!` over its inbox and any
other future: a timer, a socket, a stream.
