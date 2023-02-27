/*!
Zestors is a fast and flexible actor-framework for creating fault-tolerant Rust applications.

# Documentation
All items are self-documented, but for a new user it is recommended to go skim through the docs in
the following order. Every module gives an overview of what is contained inside, introduces some 
new concepts and then gives an example on how to use it.
- [`messaging`] - Defining a protocol in order to send messages.
- [`actor_ref`] - Interacting with an actor through it's child or address.
- [`actor_type`] - Specifying the type of an actor statically and dynamically.
- [`spawning`] - Spawning of actors.
- [`handler`] - A simpler way to write your actors.
- [`supervision`] - Supervision of actors.
- [`distribution`] - todo
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
    pub use crate::distribution::*;
}
pub(crate) mod _priv;
#[allow(unused)]
use crate::all::*;

pub use zestors_codegen::{protocol, Envelope, Message};
pub use async_trait::async_trait;
