#![doc = include_str!("../../README.md")]

pub use zestors_core::actor_type;
pub use zestors_core::config;
pub use zestors_core::error;
pub use zestors_core::messaging;
pub use zestors_core::process;
pub use zestors_request as request;

pub use zestors_codegen::{protocol, Message};
pub use zestors_core::{DynAccepts, DynAddress};

mod prelude {
    pub use crate::{
        actor_type::{Accepts, ActorType},
        process::{spawn, spawn_many, spawn_one, Address, Child, ChildPool},
        protocol, DynAccepts, DynAddress, Message,
    };
}
