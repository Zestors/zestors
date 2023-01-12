#![doc = include_str!("../docs/lib.md")]

//------------------------------------------------------------------------------------------------
//  Public API
//------------------------------------------------------------------------------------------------

pub mod channel;
pub mod config;
pub mod protocol;
pub mod receiving;
pub mod sending;
pub mod spawning;
pub mod supervision;

pub use channel::*;
pub use config::*;
pub use protocol::*;
pub use receiving::*;
pub use sending::*;
pub use spawning::*;
pub use supervision::*;

//------------------------------------------------------------------------------------------------
//  Private
//------------------------------------------------------------------------------------------------

mod _priv;

#[cfg(test)]
#[macro_use]
extern crate zestors_codegen;
pub(crate) use _priv::gen;
#[cfg(test)]
pub(crate) use _priv::test_helper::*;

pub mod zestors {
    //! Module to make the zestors macro's work with zestors-core only.
    pub use crate as core;
}
