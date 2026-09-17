# zestors

[![crates.io](https://img.shields.io/crates/v/zestors.svg)](https://crates.io/crates/zestors)
[![Documentation](https://docs.rs/zestors/badge.svg)](https://docs.rs/zestors)

`zestors` is an actor framework for Rust with Erlang/OTP-style supervision.

This is the facade crate: it re-exports the `zestors-*` workspace crates as
modules (`zestors::interface`, `zestors::runtime`, `zestors::actor`,
`zestors::supervision`, `zestors::supervisor`, `zestors::api_server`) and
collects the commonly used items in `zestors::prelude`. Depend on this
crate rather than the individual `zestors-*` crates directly.

See the [crate documentation](https://docs.rs/zestors) for a guided
walkthrough, from defining your first message to building a supervision
tree.
