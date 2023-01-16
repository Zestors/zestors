//! # Default
//! Attached with an abort-timer of `1 sec`.
//!
//! The capacity is `unbounded`, with `exponential` backoff starting `5 messages` in the inbox
//! at `25 ns`, with a growth-factor of `1.3`.

pub use zestors_core::config::*;