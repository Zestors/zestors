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

- **An actor is just a task.** There is no actor system to start: spawn an async
  closure over an `Inbox` inside any tokio runtime, and write its receive loop
  like any other async code — `select!` over messages, signals, timers and
  sockets. When you don't need that control, implement one `Handle<M>` per
  message and let `Handler` run the loop.
- **A message means the same thing to every actor.** What a message replies with
  is part of the message, not of the actor that receives it. So one message —
  `GetHealth`, say — can be asked of any actor that accepts it, and always
  answers the same way.
- **Address actors by what they accept.** An address can be narrowed to part of
  an actor's interface: `Address<Dyn<(GetHealth, GetChildren)>>`. Addresses of
  unrelated actors that share those messages have the same type and can go in
  one collection, checked at compile time; or you can send by message type and
  check at runtime. The supervision tree, the HTTP API and the inspector find
  their way around a running system this way, without knowing any actor's
  type. See [Dynamic addresses](dynamic-addresses.md).
- **Supervision in the OTP sense.** Supervisors restart actors on the same
  channel, so a name — and every address to it — stays valid across restarts.
  Restart strategies, restart budgets and child sets that change at runtime are
  all plain data.
- **Local or remote, the same code.** A `ClusterAddress` reaches an actor on this
  node or another through the same `cast` and `call`. What the network adds —
  at-most-once delivery, timeouts, lost nodes — is spelled out, not hidden.
  Clusters can be tested inside one process, on virtual time.

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
| `zestors::distr`       | `zestors-distr`         | Clustering (feature `distr`): `ClusterNode`, `Cluster`, `ClusterAddress`, remote messages. |
| `zestors::distr_quic`  | `zestors-distr-quic`    | The QUIC transport for clusters, with mutual TLS (feature `distr`).                         |
| `zestors::api_server`  | `zestors-api-server`    | An HTTP server for inspecting a running supervision tree.                                  |
| —                      | `zestors-codegen`       | The derive macros, re-exported in `zestors::prelude`.                                      |
| —                      | `zestors-distr-backend` | The transport trait, for running a cluster over something other than QUIC.                 |
| —                      | `zestors-inspector`     | A desktop GUI for the API server.                                                          |

The [API documentation](https://docs.rs/zestors) covers every type in detail.
This book explains how the pieces fit together.