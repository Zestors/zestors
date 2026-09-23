# Testing with `sim`

The `sim` feature of `zestors-distr` runs whole clusters inside one test, on a
simulated network. Enable it for tests only:

```toml
[dev-dependencies]
zestors-distr = { version = "0.3", features = ["sim"] }
tokio = { version = "1", features = ["test-util"] }
```

Together with `#[tokio::test(start_paused = true)]` the cluster runs on
*virtual time*. Timeouts and failure detection that take seconds for real take
no time, and a run with the same seed repeats exactly.

The simulation runs the real membership protocol and messaging; only the
transport is replaced. It doesn't model TLS, connection setup or reconnect
backoff, which the QUIC backend's own tests cover.

## Real nodes on a simulated network

`SimNetwork::backend(addr)` is a `Backend`, so a normal `ClusterNode` runs on it
unchanged. Any address works, as long as each node has its own.

```rust
use zestors::{distr::sim::SimNetwork, prelude::*, supervisor::Supervisor};

// In a test: #[tokio::test(start_paused = true)]
# #[tokio::main(flavor = "current_thread", start_paused = true)]
# async fn main() {
let net = SimNetwork::new(7);
let node = |name: &str, addr: &str| {
    ClusterConfig::new(name, net.backend(addr))
};

let a = ClusterNode::new(Supervisor::blueprint().rand_name(), node("node-a", "10.0.0.1:1"));
let b = ClusterNode::new(
    Supervisor::blueprint().rand_name(),
    node("node-b", "10.0.0.2:1").seed(Seed::new("node-a", "10.0.0.1:1")),
);
let (a_cluster, b_cluster) = (a.cluster(), b.cluster());
tokio::spawn(a.run());
tokio::spawn(b.run());
a_cluster.wait_for_members(1).await;

// Cut the network in two: each side declares the other down.
net.partition(&["node-a"], &["node-b"]);
a_cluster.wait_for_members(0).await;
b_cluster.wait_for_members(0).await;
# }
```

The network can also slow down (`set_latency`) and heal (`heal`).
`foca_config_mut` and `timings_mut` tune the nodes it starts.

## Membership-only nodes

`SimNetwork::start(name, addr, seeds)` starts a lighter node that takes part in
membership only, without a supervisor or messaging. `SimNode::leave()` and
`SimNode::crash()` stop it cleanly or abruptly. These are useful for testing
code that reacts to `ClusterEvent`s.

## Things to know

- All simulated nodes share the process, and therefore one `Registry`. An actor
  spawned in the test is visible to every node, so `GlobalName::new("x",
  "node-b")` finds it through node-b. Spawn each actor once, and address it
  through the node that should serve it.
- Actor names must be unique in the process. Each test runs in its own process
  under `cargo nextest`, but with `cargo test` the tests in one binary share
  one registry.
