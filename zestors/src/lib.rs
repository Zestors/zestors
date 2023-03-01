/*!
A fast and flexible actor-framework for building fault-tolerant Rust applications.

# Documentation
All items are self-documented, but for a new user it is recommended to go skim through the docs in
the following order. Every module gives an overview of what is contained inside, introduces some
new concepts and then gives an example on how to use it.
- [`messaging`] - Defining a protocol in order to send messages.
- [`actor_reference`] - Interacting with an actor through it's child or address.
- [`actor_type`] - Specifying the type of an actor statically and dynamically.
- [`spawning`] - Spawning of actors.
- [`handler`] - A simpler way to write your actors.
- [`supervision`] - Supervision of actors.
- [`distribution`] - todo
*/

pub mod actor_reference;
pub mod actor_type;
pub mod distribution;
pub mod handler;
pub mod messaging;
pub mod spawning;
pub mod supervision;

pub mod prelude {
    pub use crate::actor_reference::{ActorRefExt, Address, Child, ChildPool, Transformable};
    pub use crate::actor_type::{ActorId, Halter, Inbox, MultiHalter};
    pub use crate::handler::{Action, ExitFlow, Flow, Handler, HandlerExt};
    pub use crate::messaging::{Accepts, Envelope, Message, Rx, Tx};
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
    pub use crate::spawning::*;
    pub use crate::supervision::*;
}
pub(crate) mod _priv;

pub mod export {
    pub use async_trait::async_trait;
    pub use eyre::Report;
}

pub use zestors_codegen::{protocol, Envelope, Handler, Message};
