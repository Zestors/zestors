# zestors

[![crates.io](https://img.shields.io/crates/v/zestors.svg)](https://crates.io/crates/zestors)
[![Documentation](https://docs.rs/zestors/badge.svg)](https://docs.rs/zestors)

`zestors` is an actor framework for Rust with Erlang/OTP-style supervision.

An actor is a `tokio` task that owns an `Inbox` and processes messages —
defined as plain structs deriving `Message`, grouped per-actor into an
`Interface` — one at a time until it exits. Actors can be supervised: a
`Supervisor` starts, watches, and restarts a set of children according to a
restart strategy, and a `Node` runs a root supervisor as an entire program,
shutting it down gracefully on Ctrl+C/SIGTERM.

This repository is a Cargo workspace; most consumers should depend on the
[`zestors`](crates/zestors) facade crate, which re-exports the other crates
as modules.

## Example

A worker actor, written with `Handler`, supervised by a `Supervisor`, run as
a program with `Node`:
```rust
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
            .child(Worker.pid("worker").unwrap())
            .rand_pid(),
    );

    // Starts the supervisor, restarts it on error, and shuts it down
    // gracefully on Ctrl+C/SIGTERM.
    node.run().await.unwrap();
}
```

## Learn more

The [`zestors` crate docs](https://docs.rs/zestors) are the main
documentation: they walk through defining messages, spawning actors,
sending and receiving, and building supervision trees, with runnable
examples for each step. Each workspace crate also documents the layer it
provides — see the crate list below.

## Workspace crates

| Crate                                       | What it provides                                                                                 |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| [`zestors`](crates/zestors)                 | Facade crate re-exporting the others; start here.                                                |
| [`zestors-interface`](crates/interface)     | `Message`/`Interface`: the vocabulary for defining what an actor accepts.                        |
| [`zestors-runtime`](crates/runtime)         | The actor runtime: `Inbox`, `Address`, `Pid`, `Registry`, signals.                               |
| [`zestors-actor`](crates/actor)             | `Handler`/`Actor`: declarative and low-level ways to implement an actor.                         |
| [`zestors-supervision`](crates/supervision) | `ChildSpec`/`ChildConfig`/`RestartIntensity`: the supervisor's vocabulary.                       |
| [`zestors-supervisor`](crates/supervisor)   | The `Supervisor` and `Node` actors that use it.                                                  |
| [`zestors-api-server`](crates/api-server)   | HTTP introspection for a running actor tree.                                                     |
| [`zestors-codegen`](crates/codegen)         | The `#[derive(Message)]`/`#[derive(Interface)]` proc macros.                                     |
| [`zestors-inspector`](crates/inspector)     | A GUI that visualizes the tree data `zestors-api-server` exposes (not re-exported by `zestors`). |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
