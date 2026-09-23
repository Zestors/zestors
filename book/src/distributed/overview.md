# Distributed mode (experimental)

Several `zestors` programs — *nodes* — can form a *cluster*. An actor on one node
can message an actor on another, using the same `cast` and `call` as for a local
actor.

> **Not production ready.** Distributed mode is new. Its APIs are bound to
> change, and there will be bugs.

## The moving parts

- **A node** is one program in the cluster. `ClusterNode` runs it: it does
  everything `Node` does (runs the root supervisor, and shuts down on
  Ctrl+C/SIGTERM), and also joins the cluster.
- **Membership** is tracked with SWIM, a gossip protocol (via
  [`foca`](https://docs.rs/foca)). Each node learns who else is up, and notices
  when a node leaves or crashes. `Cluster` is the handle to this view.
- **A backend** carries the bytes between nodes. `Quic` from `zestors-distr-quic`
  is the one to use: QUIC with mutual TLS, so only nodes with a certificate from
  your CA can join. Other transports implement the `Backend` trait.
- **A `GlobalName`** names an actor anywhere in the cluster: an actor's `Name`
  plus the node it is on, written `name@node`.
- **A `ClusterAddress`** is a reference to an actor in the cluster, on this node
  or another. It sends messages with `ClusterAccepts` (`cast`, `call`, …) and
  operates on the actor with `ClusterActorOps` (signals, status, `monitor_*`).
- **A remote message** is a message that can cross the network. It has a
  `StableId` that names it the same way on every node, and it can be encoded,
  usually with serde.

## From a local program to a distributed one

1. **Enable the `distr` feature** of `zestors`, which adds the `distr` and
   `distr_quic` modules and their items in the prelude:
   `zestors = { version = "0.3", features = ["distr"] }`.
2. **Make the messages remote.** Derive `StableId` and `Serialize`/`Deserialize`
   on each message that crosses the network, and give it an id:
   `#[msg(id = "<uuid>")]`. Its reply type must be serializable too. See
   [Remote messages](remote-messages.md).
3. **Replace `Node` with `ClusterNode`,** configured with a `ClusterConfig`: the
   node's name, the backend, and the seeds to join through. See
   [Running a cluster](running-a-cluster.md).
4. **Register what each node accepts.** A node that hosts an actor registers the
   messages other nodes may send to it: `config.register::<Msg>()`. Sending needs
   no registration.
5. **Address actors by `GlobalName`.** `cluster.address::<I>(GlobalName::new("name",
   "node")).await` gives a `ClusterAddress<I>`. Every actor registered under a
   `Name` is reachable. See [Addressing and sending](addressing.md).
6. **Handle the network's failures.** A remote call can fail in ways a local one
   can't: the node is gone, a timeout expires, the message is too large. See
   [Delivery and failure](delivery.md).

## A complete example

Two nodes on the simulated network, which runs in one process on virtual time
(see [Testing with `sim`](testing.md)). With QUIC, only the backend passed to
`ClusterConfig::new` changes.

```rust
use serde::{Deserialize, Serialize};
use zestors::{
    distr::sim::SimNetwork,
    interface::{Envelope, Interface, Message},
    prelude::*,
    runtime::spawn,
    supervisor::Supervisor,
};

#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = u32, id = "b5a4c0de-0000-4000-8000-000000000001")]
struct Double(u32);

#[derive(Interface, Debug)]
enum CalcInterface {
    Double(Envelope<Double>),
}

# #[tokio::main(flavor = "current_thread", start_paused = true)]
# async fn main() {
let net = SimNetwork::new(1);

// node-b hosts the actor, so it registers the message it accepts.
let a = ClusterNode::new(
    Supervisor::blueprint().rand_name(),
    ClusterConfig::new("node-a", net.backend("10.0.0.1:7000")),
);
let b = ClusterNode::new(
    Supervisor::blueprint().rand_name(),
    ClusterConfig::new("node-b", net.backend("10.0.0.2:7000"))
        .seed(Seed::new("node-a", "10.0.0.1:7000"))
        .register::<Double>(),
);

// Nodes on one simulated network share a process, so this actor is spawned
// once and served by node-b.
let _calc = spawn(Name::new_static("calc"), |mut inbox: Inbox<CalcInterface>| async move {
    while let Some(CalcInterface::Double(envelope)) = inbox.recv().await {
        let n = envelope.msg.0;
        let _ = envelope.reply(n * 2);
    }
    Ok(())
})
.unwrap();

let cluster = a.cluster();
tokio::spawn(a.run());
tokio::spawn(b.run());
cluster.wait_for_members(1).await;

let calc = cluster
    .address::<CalcInterface>(GlobalName::new("calc", "node-b"))
    .await
    .unwrap();
assert!(calc.is_remote());
assert_eq!(calc.call(Double(21)).await.unwrap(), 42);
# }
```

`crates/zestors/examples/remote.rs` is the same over real QUIC, and
`crates/zestors/examples/cluster.rs` runs one node per terminal so you can watch
nodes join and leave.
