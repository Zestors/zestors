# zestors-supervision

[![crates.io](https://img.shields.io/crates/v/zestors-supervision.svg)](https://crates.io/crates/zestors-supervision)
[![Documentation](https://docs.rs/zestors-supervision/badge.svg)](https://docs.rs/zestors-supervision)

Shared building blocks for [`zestors`](https://crates.io/crates/zestors)
supervision trees, in the OTP sense: `ChildSpec`, `ChildConfig`, and
`RestartIntensity`, plus the `GetChildren`/`GetHealth` queries and
`SupervisionTree`, which let any actor take part in an inspectable tree. The `Supervisor` actor that actually starts, monitors,
and restarts children lives in
[`zestors-supervisor`](https://crates.io/crates/zestors-supervisor).

Part of the [`zestors`](https://crates.io/crates/zestors) actor framework —
see the [zestors book](https://zestors.github.io/zestors/) for a guided
introduction.
