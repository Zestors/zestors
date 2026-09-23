# zestors-distr

[![crates.io](https://img.shields.io/crates/v/zestors-distr.svg)](https://crates.io/crates/zestors-distr)
[![Documentation](https://docs.rs/zestors-distr/badge.svg)](https://docs.rs/zestors-distr)

Clustering for the [`zestors`](https://crates.io/crates/zestors) actor
framework: `ClusterNode` runs a program as a node of a cluster, membership is
tracked with SWIM gossip, and a `ClusterAddress` sends messages to an actor on
this node or any other. Messages that cross the network derive `StableId` and
serde.

Features: `auto-register` (register every remote message in the binary at
once) and `sim` (run whole clusters inside a test, on virtual time).

Part of the [`zestors`](https://crates.io/crates/zestors) actor framework —
see the [zestors book](https://zestors.github.io/zestors/) for a guided
introduction.
