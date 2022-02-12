#![feature(try_trait_v2)]
#![feature(associated_type_defaults)]

pub mod abort;
pub mod action;
pub mod actor;
pub mod address;
pub mod errors;
pub mod flows;
pub mod messaging;
pub mod packet;
pub mod process;
pub mod state;
pub mod inbox;

pub use anyhow::Error as AnyhowError;
pub use flows::{InitFlow, ExitFlow, MsgFlow, ReqFlow};
pub use actor::{Actor, ExitReason, Spawn, spawn};
pub use address::{Address, Addressable, RawAddress};

pub mod derive {
    pub use zestors_codegen::{Address, Addressable};
}