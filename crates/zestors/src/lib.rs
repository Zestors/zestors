//! `zestors` is an actor framework with Erlang/OTP-style supervision.
//!
//! This crate is a facade: it re-exports the workspace crates as modules and
//! collects the commonly used items in [`prelude`].
//!
//! # Reading Order
//!
//! | Module | What it provides |
//! |---|---|
//! | [`interface`] | The vocabulary: derive `Message` for each message type and `Interface` for the set of messages an actor accepts. |
//! | [`runtime`] | The delivery machinery: actors receive through `Inbox`, everyone else sends through `Address`/`StrongAddress`, and actors are looked up by `Pid` in `Registry`. |
//! | [`actor`] | The actors: implement `Handler` (per-message handlers) or `Actor` (the full event loop), then package either in an `ActorBlueprint`. |
//! | [`supervision`] | The supervisor's vocabulary: `ChildSpec` pairs a blueprint with its `ChildConfig`, and `RestartIntensity` bounds how often a child may restart. |
//! | [`supervisor`] | The supervision actors: `Supervisor` starts, watches, and restarts children per a `SupervisionStrategy`; `Node` runs a root supervisor as a whole program. |
//! | [`api_server`] | HTTP introspection: `ApiServer` exposes `/processes`, `/snapshots`, and `/health` for a running tree. |
//! | `zestors_inspector` | The `zestors-inspector` GUI — a separate workspace crate, not re-exported here — visualizes the same tree data that [`api_server`] exposes. |

pub use zestors_api_server as api_server;

/// The items you usually need, re-exported from each sub-crate plus the
/// codegen macros.
pub mod prelude {
    pub use zestors_actor::prelude::*;
    #[expect(unused_imports)]
    pub use zestors_api_server::prelude::*;
    pub use zestors_codegen::{HandlerInterface, Interface, Message};
    pub use zestors_interface::prelude::*;
    pub use zestors_runtime::prelude::*;
    pub use zestors_supervision::prelude::*;
    pub use zestors_supervisor::prelude::*;
}

pub use zestors_actor as actor;
pub use zestors_interface as interface;
pub use zestors_runtime as runtime;
pub use zestors_supervision as supervision;
pub use zestors_supervisor as supervisor;
