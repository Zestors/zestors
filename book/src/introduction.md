# Introduction

`zestors` is an actor framework for Rust with Erlang/OTP-style supervision and
clustering.

An actor is a [`tokio`](https://tokio.rs) task that owns an `Inbox` and
processes the messages sent to it. Messages are plain structs deriving
`Message`, and the set of messages an actor accepts is its `Interface`. Actors
can be supervised: a `Supervisor` starts, watches and restarts its children
according to a restart strategy. Nodes can form a cluster, and an actor on
another node is messaged the same way as one on this node.

## Ideas behind it

- **Messages define the contract.** A message says what it replies with, so an
  actor's interface — the set of messages it accepts — says exactly what can be
  asked of it and what comes back.
- **Bring your own event loop.** Implementing a `Handle<M>` per message is enough
  for most actors. When you need more control, you write the loop over the
  `Inbox` yourself, interleaving messages, signals and any other future.
- **Use only what you need.** The pieces are separate crates, and each layer
  builds only on the one below it. You can write your own supervisor, or skip
  the `Actor` and `Blueprint` traits entirely and spawn a closure.
- **Dynamic addresses.** An address can be narrowed to a subset of the actor's
  interface, like `Address<Dyn<(GetHealth, GetChildren)>>`. Addresses of
  unrelated actors that share those messages then have the same type, and can
  go in one collection — still strongly typed. The supervision tree and its
  introspection are built on this; see [Dynamic addresses](dynamic-addresses.md).

## The crates

Most programs depend only on the `zestors` crate, which re-exports the others as
modules.

| Module                 | Crate                   | What it provides                                                                           |
| ---------------------- | ----------------------- | ------------------------------------------------------------------------------------------ |
| `zestors::interface`   | `zestors-interface`     | `Message` and `Interface`: what an actor accepts and how it replies.                       |
| `zestors::runtime`     | `zestors-runtime`       | `Inbox`, `Address`, `Child`, `Name`, the `Registry`, signals and statuses.                 |
| `zestors::actor`       | `zestors-actor`         | `Handler` (one handler per message) and `Actor` (a full event loop), and `Blueprint`.      |
| `zestors::supervision` | `zestors-supervision`   | `ChildSpec`, `ChildConfig`, `RestartIntensity`, and the `GetChildren`/`GetHealth` queries. |
| `zestors::supervisor`  | `zestors-supervisor`    | The `Supervisor` actor, and `Node` to run one as a program.                                |
| `zestors::distr`       | `zestors-distr`         | Clustering: `ClusterNode`, `Cluster`, `ClusterAddress`, remote messages.                   |
| `zestors::distr_quic`  | `zestors-distr-quic`    | The QUIC transport for clusters, with mutual TLS.                                          |
| `zestors::api_server`  | `zestors-api-server`    | An HTTP server for inspecting a running supervision tree.                                  |
| —                      | `zestors-codegen`       | The derive macros, re-exported in `zestors::prelude`.                                      |
| —                      | `zestors-distr-backend` | The transport trait, for running a cluster over something other than QUIC.                 |
| —                      | `zestors-inspector`     | A desktop GUI for the API server.                                                          |

The [API documentation](https://docs.rs/zestors) covers every type in detail.
This book explains how the pieces fit together.
d