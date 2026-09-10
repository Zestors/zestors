//! Messaging module for the Zestors runtime.
//!
//! This module provides the core interface functionality for the Zestors runtime, including message types, envelopes, and interfaces. It defines the traits and structures necessary for sending and receiving messages between different components of the system.

pub mod prelude {
    pub use crate::{
        Envelope, Message,
        oneshot::{Request, Response},
    };
}

mod interface;

pub use interface::*;

mod message;
pub use message::*;

mod oneshot;
pub use oneshot::*;

mod envelope;
pub use envelope::*;

use type_sets::AsTypeSet;

// pub mod errors;
