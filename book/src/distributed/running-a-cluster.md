# Running a cluster

## `ClusterNode`

`ClusterNode` is `Node` with cluster membership added. It runs a root supervisor
the same way, shuts down on Ctrl+C/SIGTERM the same way, and additionally:

- starts the backend and joins the cluster before the supervisor starts;
- serves the messages registered in its `ClusterConfig`;
- announces its departure on the way out, so the other nodes see it leave at
  once instead of timing it out.

```rust,ignore
{{#include ../../../crates/zestors/examples/cluster.rs:config}}
```

Take `node.cluster()` before calling `run()`, which consumes the node. The
`Cluster` handle is cheap to clone, and works before the node has started: it
then reports no members, and sends fail with `CastFailure::NotRunning`.

To add clustering to a `Node` you have already configured (for example with a
custom exit watcher), use `ClusterNode::from_node(node, config)`.

## `ClusterConfig`

`ClusterConfig::new(name, backend)` takes the node's name and its transport.
The rest is optional:

| Method | Default | What it does |
| --- | --- | --- |
| `seed(Seed::new(name, addr))` | none | A node to contact when joining. Add several for redundancy. A node without seeds waits for others to contact it. |
| `register::<M>()` | — | Accept `M` from other nodes. See [Remote messages](remote-messages.md). |
| `auto_register()` | — | Register every remote message in the binary (feature `auto-register`). |
| `advertise(addr)` | the bound address | The address other nodes should use to reach this one, when it differs from the one it listens on (NAT, containers). |
| `call_timeout(d)` | 30 s | How long a call to an actor on *another* node waits for its reply. |
| `lanes(n)` | 4 | Streams per peer. See [Delivery and failure](delivery.md#ordering). |
| `expected_size(n)` | — | Roughly how many nodes to expect; tunes failure detection. |
| `generation_store(path)` | none | A file that keeps the node's incarnation number increasing across restarts, even if the clock is set back. |
| `timings`, `link_timings`, `foca_config`, `rng_seed` | — | Low-level tuning of membership and connections. |

Nodes don't have to agree on any of these.

## Node names

A node's name (`NodeName`) identifies it in the cluster, and is the node part of
every `GlobalName` that points at it. With QUIC, it is also the TLS server name
that peers dial, so it must be a valid DNS name — `node-a` or
`worker-3.cluster.internal` — and it must match the node's certificate.

## QUIC and TLS

`Quic::new(bind, tls)` listens on `bind` (a UDP address) with the identity
`tls`. For production, use mutual TLS with your own CA:

```rust,no_run
use zestors::prelude::*;
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let tls = Tls::from_pem(
    &std::fs::read("ca.pem")?,     // the cluster CA that peers must chain to
    &std::fs::read("node-a.pem")?, // this node's certificate (chain)
    &std::fs::read("node-a.key")?, // this node's private key
)?;
let config = ClusterConfig::new("node-a", Quic::new("0.0.0.0:7000".parse()?, tls));
# let _ = config; Ok(())
# }
```

- Every node presents a certificate signed by the CA, and verifies its peers'
  certificates. Holding a certificate from the CA is what lets a node join.
- A node's certificate must carry **exactly one DNS name**, the node's name.
  What a peer is called in the cluster is what its certificate says.

For example, with `openssl`:

```bash
# The cluster CA, once.
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
  -keyout ca.key -out ca.pem -days 3650 -subj "/CN=my-cluster-ca"

# A certificate for the node named node-a.
openssl req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
  -keyout node-a.key -out node-a.csr -subj "/CN=node-a"
openssl x509 -req -in node-a.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
  -out node-a.pem -days 365 -extfile <(printf "subjectAltName=DNS:node-a")
```

`Tls::insecure_dev()` skips all verification: each node makes a self-signed
certificate, and anyone who can reach the port can join as any node. The
traffic is still encrypted. Use it only for local development and examples.

## Starting and stopping

- `NodeStatus` (from `cluster.status()`, or `wait_for_status`) moves from
  `Starting` to `Up`, then to `Leaving` on the way out.
- A node that the rest of the cluster declared down, and that can't rejoin, is
  `Defunct`. It keeps running its supervisor, but no longer takes part in the
  cluster. Watch for it and restart the process.
- To stop a node from inside the program, signal its root supervisor, as with
  `Node`. `run()` then leaves the cluster and returns.
