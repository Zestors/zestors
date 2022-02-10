#![feature(try_trait_v2)]
#![feature(associated_type_defaults)]

pub mod abort;
pub mod action;
pub mod actor;
pub mod address;
pub mod callable;
pub mod errors;
pub mod flows;
pub mod messaging;
pub mod packets;
pub mod process;
pub mod sending;
pub mod state;

pub use anyhow::Error as AnyhowError;

pub use flows::{InitFlow, ExitFlow, Flow, ReqFlow};
pub use actor::{Actor, ExitReason, Spawn, spawn};
pub use callable::{Callable};