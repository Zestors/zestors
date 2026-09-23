# Feature flags

## `zestors`

| Feature | Default | Enables |
| --- | --- | --- |
| `distr` | off | [Distributed mode](../distributed/overview.md): the `zestors::distr` and `zestors::distr_quic` modules, and their items in the prelude (`ClusterNode`, `ClusterAccepts`, `StableId`, `Quic`, `Tls`, …). Not production ready yet. |
| `auto-register` | off | `ClusterConfig::auto_register`, which registers every remote message in the binary. Implies `distr`. |

Without `distr`, the cluster crates and the QUIC stack (`quinn`, `rustls`)
aren't compiled at all.

## `zestors-distr`

| Feature | Default | Enables |
| --- | --- | --- |
| `auto-register` | off | `ClusterConfig::auto_register`, collecting messages at link time with [`inventory`](https://docs.rs/inventory). |
| `sim` | off | The `sim` module: in-process clusters on a simulated network. Meant for tests; see [Testing with `sim`](../distributed/testing.md). |

## `_ra`

Several crates define a `_ra` feature. It is only there so that rust-analyzer in
this workspace (see `.vscode/settings.json`) checks feature-gated code. Don't
enable it in your own project.
