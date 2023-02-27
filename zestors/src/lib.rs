/*!
Zestors is a dynamic actor-framework built for Rust applications.

# Documentation
All modules are self-documented. For a new user it is recommended to read the documentation in
the following order.
- [`messaging`] - Using a protocol to send messages.
- [`monitoring`] - Monitoring a running actor.
- [`spawning`] - Spawning of actors.
- [`channel`] - Using different channels for your actors.
- [`supervision`] : todo
- [`distribution`] : todo
*/

pub mod actor;
pub mod channel;
pub mod distribution;
pub mod messaging;
pub mod monitoring;
pub mod spawning;
pub mod supervision;
pub mod handler;

mod all {
    pub use crate::handler::*;
    pub use crate::channel::{halter::*, inbox::*, multi_halter::*, *};
    pub use crate::messaging::*;
    pub use crate::monitoring::*;
    pub use crate::spawning::*;
    pub use crate::supervision::*;
    pub use crate::Accepts;
}
pub(crate) mod _priv;
#[allow(unused)]
use crate::all::*;

pub use zestors_codegen::{protocol, Envelope, Message};
pub use async_trait::async_trait;
