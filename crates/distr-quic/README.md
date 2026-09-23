# zestors-distr-quic

[![crates.io](https://img.shields.io/crates/v/zestors-distr-quic.svg)](https://crates.io/crates/zestors-distr-quic)
[![Documentation](https://docs.rs/zestors-distr-quic/badge.svg)](https://docs.rs/zestors-distr-quic)

A QUIC backend for [`zestors-distr`](https://crates.io/crates/zestors-distr)
clusters, with mutually authenticated nodes: every node presents a certificate
from the cluster's CA, carrying its node name.

Part of the [`zestors`](https://crates.io/crates/zestors) actor framework —
see the [zestors book](https://zestors.github.io/zestors/) for a guided
introduction.
