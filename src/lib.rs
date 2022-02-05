#![feature(try_trait_v2)]
#![feature(associated_type_defaults)]
#![feature(generic_associated_types)]

use std::{marker::PhantomData, mem::size_of_val};

use futures::Future;

pub mod actor;
pub mod address;
pub mod callable;
pub mod flow;
pub mod messaging;
pub mod packets;
pub mod action;
pub mod state;
pub mod sending;
pub use anyhow::Error as AnyhowError;