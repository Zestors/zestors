//! Messaging module for the Zestors runtime.
//!
//! This module provides the core messaging functionality for the Zestors runtime, including message types, envelopes, and interfaces. It defines the traits and structures necessary for sending and receiving messages between different components of the system.

pub mod prelude {
    pub use crate::{
        Envelope, Message,
        oneshot::{Rx, Tx},
    };
}

mod interface;

use std::marker::PhantomData;

pub use interface::*;

mod message;
pub use message::*;

pub mod oneshot;
pub(crate) use oneshot::*;

mod envelope;
pub use envelope::*;

use type_sets::AsTypeSet;

// pub mod errors;
