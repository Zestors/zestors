/*!
Zestors is a dynamic actor-framework built for Rust applications.

# Documentation
All modules are self-documented. For a new user it is recommended to read the documentation in
the following order.
- [`messaging`] - Defining a protocol to send messages.
- [`actor_ref`] - Interacting with an actor through it's child or address.
- [`actor_type`] - Specifying the type of an actor.
- [`spawning`] - Spawning of actors.
- [`handler`] - A simple way to write actors.
- [`supervision`] : Supervision of actors.
- [`distribution`] : todo
*/

pub mod actor_type;
pub mod distribution;
pub mod messaging;
pub mod actor_ref;
pub mod spawning;
pub mod supervision;
pub mod handler;

mod all {
    pub use crate::handler::*;
    pub use crate::actor_type::{halter::*, inbox::*, multi_halter::*, *};
    pub use crate::messaging::*;
    pub use crate::actor_ref::*;
    pub use crate::spawning::*;
    pub use crate::supervision::*;
    pub use crate::Accepts;
}
pub(crate) mod _priv;
#[allow(unused)]
use crate::all::*;

pub use zestors_codegen::{protocol, Envelope, Message};
pub use async_trait::async_trait;
