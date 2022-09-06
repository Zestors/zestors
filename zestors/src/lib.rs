#![doc = include_str!("../../README.md")]

pub mod actor_type {
    #![doc = include_str!("../../docs/actor_type.md")]

    pub use zestors_core::actor_type::*;
}

pub mod config {
    #![doc = include_str!("../../docs/config.md")]

    pub use zestors_core::config::*;
}

pub mod error {
    #![doc = include_str!("../../docs/error.md")]

    pub use zestors_core::error::*;
}

pub mod process {
    #![doc = include_str!("../../docs/process.md")]

    pub use zestors_core::process::*;
}

pub mod protocol {
    #![doc = include_str!("../../docs/protocol.md")]

    pub use zestors_core::messaging::*;
}

pub mod request {
    #![doc = include_str!("../../docs/request.md")]

    pub use zestors_request::*;
}

// pub mod actor {
//     #![doc = include_str!("../../docs/actor.md")]

//     pub use zestors_extra::actor::*;
//     pub use zestors_extra::event::*;
// }

mod prelude {
    pub use crate::{
        actor_type::{Accepts, ActorType},
        process::{spawn, spawn_many, spawn_one, Address, Child, ChildPool},
        DynAccepts, DynAddress,
    };
}

pub use zestors_codegen::{protocol, Message};
pub use zestors_core::{DynAccepts, DynAddress};
