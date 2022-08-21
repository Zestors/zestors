#![doc = include_str!("../../README.md")]

//------------------------------------------------------------------------------------------------
//  Public modules
//------------------------------------------------------------------------------------------------

pub mod actor;
pub mod actor_type;
pub mod config;
pub mod error;
pub mod protocol;
pub mod request;

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
        actor::{spawn, spawn_many, spawn_one, Addr, Child, ChildPool},
        actor_type::{Accepts, ActorType},
        DynAccepts, DynAddress,
    };
}
