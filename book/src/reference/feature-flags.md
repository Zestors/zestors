# Feature flags

## `zestors`

| Feature | Default | Enables |
| --- | --- | --- |
| `auto-register` | off | `ClusterConfig::auto_register`, which registers every remote message in the binary. Same as `zestors-distr/auto-register`. |

The `zestors` crate always includes the cluster crates, `zestors-distr` and
`zestors-distr-quic`. The QUIC stack (`quinn`, `rustls`) is part of every build.
A program that never runs a cluster and wants to avoid compiling it can depend
on the crates it needs directly (`zestors-runtime`, `zestors-actor`, …), setting
the derives' path attributes as in [Derive attributes](derive-attributes.md).

## `zestors-distr`

| Feature | Default | Enables |
| --- | --- | --- |
| `auto-register` | off | `ClusterConfig::auto_register`, collecting messages at link time with [`inventory`](https://docs.rs/inventory). |
| `sim` | off | The `sim` module: in-process clusters on a simulated network. Meant for tests; see [Testing with `sim`](../distributed/testing.md). |

## `_ra`

Several crates define a `_ra` feature. It is only there so that rust-analyzer in
this workspace (see `.vscode/settings.json`) checks feature-gated code. Don't
enable it in your own project.
