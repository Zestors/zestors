# Getting started

Add `zestors` and `tokio` to your `Cargo.toml`. Actors written with `Handler`
return a [`rootcause::Report`](https://docs.rs/rootcause) as their error, so
most programs need `rootcause` as well:

```toml
[dependencies]
zestors = "0.3"
tokio = { version = "1", features = ["full"] }
rootcause = "0.13"
```

Clustering — [distributed mode](distributed/overview.md) — is behind the
`distr` feature, which is off by default. Its messages are usually serialized
with `serde`:

```toml
zestors = { version = "0.3", features = ["distr"] }
serde = { version = "1", features = ["derive"] }
```

## A first actor

A counter that can be incremented and asked for its count. `Increment` expects
no reply; `GetCount` replies with a `u32`.

```rust
use zestors::interface::{Envelope, Interface, Message};
use zestors::prelude::*;
use zestors::runtime::spawn_rand;

#[derive(Message, Debug)]
struct Increment;

#[derive(Message, Debug)]
#[msg(reply = u32)]
struct GetCount;

#[derive(Interface, Debug)]
enum CounterInterface {
    Increment(Envelope<Increment>),
    GetCount(Envelope<GetCount>),
}

#[tokio::main]
async fn main() {
    let child = spawn_rand(|mut inbox: Inbox<CounterInterface>| async move {
        let mut count = 0;
        while let Some(msg) = inbox.recv().await {
            match msg {
                CounterInterface::Increment(_) => count += 1,
                CounterInterface::GetCount(envelope) => {
                    let _ = envelope.reply(count);
                }
            }
        }
        Ok(())
    });

    for _ in 0..5 {
        child.cast(Increment).await.unwrap();
    }
    // One actor's messages are handled in order, so the count includes all
    // five increments.
    assert_eq!(child.call(GetCount).await.unwrap(), 5);

    child.signal_shutdown();
}
```

The next chapters take this apart:

- [Messages and interfaces](messages.md): the two derives.
- [Spawning and messaging actors](actors.md): `spawn_rand`, `Child`, `cast` and `call`.
- [Lifecycle and signals](lifecycle.md): what `signal_shutdown` does, and when.
- [Handler actors](handler.md): the same counter without the hand-written loop.

To run a program under a supervision tree instead of spawning actors by hand,
see [Supervision](supervision.md). To run it across several machines, see
[Distributed mode](distributed/overview.md).

The workspace also has runnable examples in `crates/zestors/examples`:

```sh
# A supervision tree, with the HTTP API on :8080
cargo run -p zestors --example supervision

# Two cluster nodes in one process
cargo run -p zestors --features distr --example remote

# One cluster node per terminal
cargo run -p zestors --features distr --example cluster -- node-a 127.0.0.1:7001
```
