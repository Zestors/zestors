# zestors

[![crates.io](https://img.shields.io/crates/v/zestors.svg)](https://crates.io/crates/zestors)
[![Documentation](https://docs.rs/zestors/badge.svg)](https://docs.rs/zestors)
[![Book](https://img.shields.io/badge/book-zestors-blue)](https://zestors.github.io/zestors/)

`zestors` is an actor framework for Rust with Erlang/OTP-style supervision and
clustering.

An actor is a `tokio` task that owns an `Inbox` and handles the messages sent to
it. Messages are plain structs deriving `Message`, and the set of messages an
actor accepts is its `Interface`. A `Supervisor` starts, watches and restarts
its children according to a restart strategy, and a `Node` runs a supervision
tree as a program. Nodes can join a cluster, where an actor on another node is
messaged just like a local one.

- **The interface defines the contract.** A message says what it replies with,
  so an actor's interface says exactly what can be asked of it.
- **Bring your own event loop.** Implement one `Handle<M>` per message, or write
  the receive loop yourself and interleave messages, signals and any other
  future.
- **Use only what you need.** Each layer is a separate crate. Write your own
  supervisor, or skip the `Actor` and `Blueprint` traits and spawn a closure.
- **Dynamic addresses.** An address can be narrowed to part of an actor's
  interface: `Address<Dyn<(GetHealth, GetChildren)>>`. Addresses of different
  kinds of actor then share one type, still strongly typed. This is how the
  supervision tree is inspected at runtime: any actor that accepts
  `GetChildren` is part of it.
- **Clustering.** Nodes discover each other over SWIM gossip and talk over QUIC
  with mutual TLS. A `ClusterAddress` reaches an actor wherever it runs.

## Example

A worker, written as a `Handler`, supervised by a `Supervisor` that runs as a
program with `Node`:

```rust,no_run
use zestors::interface::{Envelope, Interface, Message};
use zestors::prelude::*;
use zestors::supervisor::{Node, Supervisor};

#[derive(Message, Debug)]
struct Ping;

#[derive(Interface, HandlerInterface, Debug)]
enum WorkerInterface {
    Ping(Envelope<Ping>),
}

#[derive(Debug, Clone)]
struct Worker;

impl Handler for Worker {
    type Interface = WorkerInterface;
}

impl Handle<Ping> for Worker {
    async fn handle(
        &mut self,
        _ctx: HandlerContext<'_, Self>,
        _msg: Ping,
        _req: (),
    ) -> Result<(), rootcause::Report> {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let node = Node::new(
        Supervisor::blueprint()
            .child(Worker.name("worker").unwrap())
            .rand_name(),
    );

    // Starts the supervisor, and shuts it down gracefully on Ctrl+C/SIGTERM.
    node.run().await.unwrap();
}
```

```toml
[dependencies]
zestors = "0.2"
tokio = { version = "1", features = ["full"] }
rootcause = "0.13"
```

## Learn more

- **[The zestors book](https://zestors.github.io/zestors/)**: the guide, from a
  first actor to supervision trees and clusters.
- **[API documentation](https://docs.rs/zestors)**: every type in detail.
- **Examples** in [`crates/zestors/examples`](crates/zestors/examples):
  `supervision` (a supervision tree with the HTTP API), `remote` (two cluster
  nodes in one process) and `cluster` (one node per terminal).

A proof-of-concept [inspector GUI](crates/inspector) draws a running supervision
tree; see [Observability](https://zestors.github.io/zestors/observability.html).

## Workspace crates

Most programs depend only on the [`zestors`](crates/zestors) crate, which
re-exports the others as modules.

| Crate                                           | What it provides                                                                                 |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| [`zestors`](crates/zestors)                     | The facade: re-exports the others, and a prelude. Start here.                                    |
| [`zestors-interface`](crates/interface)         | `Message`/`Interface`: what an actor accepts, and how it replies.                                |
| [`zestors-runtime`](crates/runtime)             | `Inbox`, `Address`, `Child`, `Name`, the `Registry`, signals, statuses and dynamic addresses.    |
| [`zestors-actor`](crates/actor)                 | `Handler`/`Actor`: declarative and low-level ways to implement an actor; `Blueprint`.            |
| [`zestors-supervision`](crates/supervision)     | `ChildSpec`/`ChildConfig`/`RestartIntensity`, and the `GetChildren`/`GetHealth` queries.         |
| [`zestors-supervisor`](crates/supervisor)       | The `Supervisor` actor, and `Node` to run one as a program.                                      |
| [`zestors-distr`](crates/distr)                 | Clustering: `ClusterNode`, `Cluster`, `ClusterAddress`, remote messages.                         |
| [`zestors-distr-quic`](crates/distr-quic)       | The QUIC transport for clusters, with mutual TLS.                                                |
| [`zestors-distr-backend`](crates/distr-backend) | The transport trait, to run a cluster over another network.                                      |
| [`zestors-api-server`](crates/api-server)       | An HTTP server for inspecting a running supervision tree.                                        |
| [`zestors-codegen`](crates/codegen)             | The `Message`, `Interface`, `HandlerInterface` and `StableId` derive macros.                     |
| [`zestors-inspector`](crates/inspector)         | A GUI for the data `zestors-api-server` serves (not re-exported by `zestors`).                   |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
