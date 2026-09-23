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

- **An actor is just a task.** No actor system to start: spawn an async closure
  over an `Inbox` in any tokio runtime and `select!` over whatever it needs — or
  implement one `Handle<M>` per message and let `Handler` run the loop.
- **A message means the same thing to every actor.** Its reply type belongs to
  the message, not to the receiver, so one message (`GetHealth`) can be asked of
  any actor that accepts it.
- **Address actors by what they accept.** `Address<Dyn<(GetHealth,
  GetChildren)>>` has the same type for unrelated actors that share those
  messages. The supervision tree, HTTP API and inspector explore a running
  system this way, without knowing any actor's type.
- **Supervision in the OTP sense.** Actors restart on the same channel, so names
  and addresses stay valid across restarts; strategies, restart budgets and
  dynamic child sets are plain data.
- **Local or remote, the same code.** A `ClusterAddress` reaches an actor on any
  node with the same `cast` and `call`, over QUIC with mutual TLS. The network's
  semantics are spelled out, and clusters can be tested in one process on
  virtual time.

> [!WARNING]
> **Distributed mode is not production ready.** Its APIs are bound to change,
> and there will be bugs. It is behind the `distr` feature, which is off by
> default. The rest of the framework — actors, messaging and
> supervision — is the mature part.

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
zestors = "0.3"
tokio = { version = "1", features = ["full"] }
rootcause = "0.13"
```

## Learn more

- **[The zestors book](https://zestors.github.io/zestors/)**: the guide, from a
  first actor to supervision trees and clusters.
- **[API documentation](https://docs.rs/zestors)**: every type in detail.
- **Examples** in [`crates/zestors/examples`](crates/zestors/examples):
  `supervision` (a supervision tree with the HTTP API), and, with
  `--features distr`, `remote` (two cluster nodes in one process) and `cluster`
  (one node per terminal).

A proof-of-concept [inspector GUI](crates/inspector) draws a running supervision
tree; see [Observability](https://zestors.github.io/zestors/observability.html).

## Workspace crates

Most programs depend only on the [`zestors`](crates/zestors) crate, which
re-exports the others as modules.

| Crate                                           | What it provides                                                                              |
| ----------------------------------------------- | --------------------------------------------------------------------------------------------- |
| [`zestors`](crates/zestors)                     | The facade: re-exports the others, and a prelude. Start here.                                 |
| [`zestors-interface`](crates/interface)         | `Message`/`Interface`: what an actor accepts, and how it replies.                             |
| [`zestors-runtime`](crates/runtime)             | `Inbox`, `Address`, `Child`, `Name`, the `Registry`, signals, statuses and dynamic addresses. |
| [`zestors-actor`](crates/actor)                 | `Handler`/`Actor`: declarative and low-level ways to implement an actor; `Blueprint`.         |
| [`zestors-supervision`](crates/supervision)     | `ChildSpec`/`ChildConfig`/`RestartIntensity`, and the `GetChildren`/`GetHealth` queries.      |
| [`zestors-supervisor`](crates/supervisor)       | The `Supervisor` actor, and `Node` to run one as a program.                                   |
| [`zestors-distr`](crates/distr)                 | Clustering: `ClusterNode`, `Cluster`, `ClusterAddress`, remote messages.                      |
| [`zestors-distr-quic`](crates/distr-quic)       | The QUIC transport for clusters, with mutual TLS.                                             |
| [`zestors-distr-backend`](crates/distr-backend) | The transport trait, to run a cluster over another network.                                   |
| [`zestors-api-server`](crates/api-server)       | An HTTP server for inspecting a running supervision tree.                                     |
| [`zestors-codegen`](crates/codegen)             | The `Message`, `Interface`, `HandlerInterface` and `StableId` derive macros.                  |
| [`zestors-inspector`](crates/inspector)         | A GUI for the data `zestors-api-server` serves (not re-exported by `zestors`).                |

## AI policy

AI agents were used to write parts of the documentation and the implementation,
always paired with human supervision and thorough analysis of what they
produced. The core of zestors — its actors, messaging and supervision — was
crafted and coded by hand, with love and attention to detail.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
