#![doc = include_str!("../../README.md")]

//------------------------------------------------------------------------------------------------
//  Public modules
//------------------------------------------------------------------------------------------------

pub mod process;
pub mod actor_type;
pub mod config;
pub mod error;
pub mod protocol;
pub mod request;
pub mod actor;

//------------------------------------------------------------------------------------------------
//  Exports
//------------------------------------------------------------------------------------------------

pub use zestors_codegen::{protocol, Message};
pub use zestors_core::{DynAccepts, DynAddress};

//------------------------------------------------------------------------------------------------
//  Prelude
//------------------------------------------------------------------------------------------------

mod prelude {
    pub use crate::{
        process::{spawn, spawn_many, spawn_one, Address, Child, ChildPool},
        actor_type::{Accepts, ActorType},
        DynAccepts, DynAddress,
    };
}
