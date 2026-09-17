# zestors-api-server

[![crates.io](https://img.shields.io/crates/v/zestors-api-server.svg)](https://crates.io/crates/zestors-api-server)
[![Documentation](https://docs.rs/zestors-api-server/badge.svg)](https://docs.rs/zestors-api-server)

An HTTP API server actor for [`zestors`](https://crates.io/crates/zestors):
exposes introspection endpoints (processes, snapshots, health) over a
running supervision tree. The endpoints are unstable and may change with
minor version bumps.

Part of the [`zestors`](https://crates.io/crates/zestors) actor framework —
see that crate's documentation for a guided introduction.
