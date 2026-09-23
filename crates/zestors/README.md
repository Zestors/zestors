# zestors

[![crates.io](https://img.shields.io/crates/v/zestors.svg)](https://crates.io/crates/zestors)
[![Documentation](https://docs.rs/zestors/badge.svg)](https://docs.rs/zestors)
[![Book](https://img.shields.io/badge/book-zestors-blue)](https://zestors.github.io/zestors/)

`zestors` is an actor framework for Rust with Erlang/OTP-style supervision and
clustering.

This is the facade crate. It re-exports the `zestors-*` crates as modules
(`zestors::interface`, `zestors::runtime`, `zestors::actor`,
`zestors::supervision`, `zestors::supervisor`, `zestors::distr`,
`zestors::distr_quic`, `zestors::api_server`), and collects the commonly used
items, including the derive macros, in `zestors::prelude`. Depend on this crate
rather than on the individual `zestors-*` crates.

The [zestors book](https://zestors.github.io/zestors/) is the guide, from a
first actor to supervision trees and clusters. The
[API documentation](https://docs.rs/zestors) is the reference.

Features:
- `distr`: distributed mode, the `distr` and `distr_quic` modules. Off by
  default, and not production ready yet.
- `auto-register`: enables `ClusterConfig::auto_register`. Implies `distr`.
