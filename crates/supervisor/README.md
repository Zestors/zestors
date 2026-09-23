# zestors-supervisor

[![crates.io](https://img.shields.io/crates/v/zestors-supervisor.svg)](https://crates.io/crates/zestors-supervisor)
[![Documentation](https://docs.rs/zestors-supervisor/badge.svg)](https://docs.rs/zestors-supervisor)

An OTP-style `Supervisor` actor for [`zestors`](https://crates.io/crates/zestors):
starts and monitors a set of children, restarting them according to a
restart strategy. `Node` runs a root `Supervisor` as an entire program,
shutting it down gracefully on Ctrl+C/SIGTERM.

Part of the [`zestors`](https://crates.io/crates/zestors) actor framework —
see the [zestors book](https://zestors.github.io/zestors/) for a guided
introduction.
