# Custom backends

A backend is the network a cluster runs on. `Quic` is the one provided; the
`Backend` trait in `zestors-distr-backend` lets a cluster run over anything
else. The simulated network in `sim` is itself a backend.

A backend is deliberately small. It only has to:

- **connect and accept with verified identity.** `Connection::peer()` is the
  node on the other end, established by the backend from a certificate, a key
  or credentials, and never taken from the peer's word. The cluster trusts it
  completely.
- **offer independent, ordered, reliable streams.** A stall on one stream must
  not hold up the others.
- **offer unreliable datagrams, if it can.** A backend without them returns
  `DatagramError::Unsupported`, and the cluster uses streams instead.

Everything else is done by the cluster, the same for every backend: one
connection per peer, reconnecting with backoff, telling unreachable from down,
framing, ordering and routing messages.

The three traits:

| Trait | Is | Main methods |
| --- | --- | --- |
| `Backend` | the configuration, consumed on start | `start(NodeIncarnation) -> Endpoint` |
| `Endpoint` | this node on the network | `connect(addr, name)`, `accept()`, `local_addr()`, `close(grace)` |
| `Connection` | a link to one peer | `open_stream()`, `accept_stream()`, `send_datagram`, `recv_datagram`, `peer()` |

`NodeName` and `NodeAddr` are opaque to the cluster. The backend decides what a
name must look like (for QUIC, a DNS name) and how an address is resolved (for
QUIC, `host:port`).

See the [`zestors-distr-backend` API docs](https://docs.rs/zestors-distr-backend)
for the full contract, and `zestors-distr-quic` for a complete implementation.
