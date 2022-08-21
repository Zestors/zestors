//------------------------------------------------------------------------------------------------
//  Public modules
//------------------------------------------------------------------------------------------------

pub mod request;
pub mod actor;
pub mod actor_type;
pub mod protocol;
pub mod error;
pub mod config;

//------------------------------------------------------------------------------------------------
//  Root exports
//------------------------------------------------------------------------------------------------

pub use zestors_codegen::{protocol, Message};
pub use zestors_core::{Accepts, Address};

//------------------------------------------------------------------------------------------------
//  Prelude
//------------------------------------------------------------------------------------------------

mod prelude {
    pub use crate::{
        actor::{spawn, spawn_many, spawn_one, Address, Child, ChildPool},
        actor_type::{Accepts, ActorType},
        Accepts, Address,
    };
}


