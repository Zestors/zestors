//! A fast and flexible actor-framework for building fault-tolerant Rust applications.
//!
//! # Documentation
//! All items are self-documented, but for a new user it is recommended to go skim through the docs in
//! the following order. Every module gives an overview of what is contained inside, introduces some
//! new concepts and then gives an example on how to use it.
//! - [`messaging`] - Defining a protocol in order to send messages.
//! - [`actor_reference`] - Interacting with an actor through it's child or address.
//! - [`actor_type`] - Specifying the type of an actor statically and dynamically.
//! - [`spawning`] - Spawning of actors.
//! - [`handler`] - A simpler way to write your actors.
//! - [`runtime`] - Runtime configuration.
//! - [`supervision`] - (Not yet implemented)
//! - [`distribution`] - (Not yet implemented)
//!
//! # Minimal example
//! ```
#![doc = include_str!("../examples/minimal.rs")]
//! ```

pub mod actor_reference;
pub mod actor_type;
pub mod distribution;
pub mod handler;
pub mod messaging;
pub mod runtime;
pub mod spawning;
pub mod supervision;

extern crate self as zestors;

pub mod prelude {
    pub use crate::actor_reference::{ActorRefExt, Address, Child, ChildPool, Transformable};
    pub use crate::actor_type::{ActorId, Halter, Inbox, MultiHalter};
    pub use crate::handler::{
        action, Action, Event, ExitFlow, Flow, HandleMessage, Handler, HandlerExt, HandlerResult,
        RestartReason, Scheduler,
    };
    pub use crate::messaging::{Accepts, Envelope, Message, Rx, Tx};
    pub use crate::runtime::{get_default_shutdown_time, set_default_shutdown_time};
    pub use crate::spawning::{
        spawn, spawn_many, spawn_many_with, spawn_with, BackPressure, Capacity, Link,
    };
}

mod all {
    pub use crate::actor_reference::*;
    pub use crate::actor_type::*;
    pub use crate::distribution::*;
    pub use crate::handler::*;
    pub use crate::messaging::*;
    pub use crate::runtime::*;
    pub use crate::spawning::*;
    pub use crate::supervision::*;
}
#[cfg(test)]
pub(crate) mod _test;

pub mod export {
    pub use async_trait::async_trait;
    pub use eyre::Report;
}

pub use zestors_codegen::{protocol, Envelope, Handler, Message};
