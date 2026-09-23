# zestors-distr-backend

[![crates.io](https://img.shields.io/crates/v/zestors-distr-backend.svg)](https://crates.io/crates/zestors-distr-backend)
[![Documentation](https://docs.rs/zestors-distr-backend/badge.svg)](https://docs.rs/zestors-distr-backend)

The transport interface of [`zestors-distr`](https://crates.io/crates/zestors-distr)
clusters: the `Backend`, `Endpoint` and `Connection` traits, `NodeName` and
`NodeAddr`. Implement `Backend` to run a cluster over another network;
[`zestors-distr-quic`](https://crates.io/crates/zestors-distr-quic) is the QUIC
implementation.

Part of the [`zestors`](https://crates.io/crates/zestors) actor framework —
see the [zestors book](https://zestors.github.io/zestors/) for a guided
introduction.
