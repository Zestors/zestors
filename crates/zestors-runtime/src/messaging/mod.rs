//! Messaging module for the Zestors runtime.
//!
//! This module provides the core messaging functionality for the Zestors runtime, including message types, envelopes, and interfaces. It defines the traits and structures necessary for sending and receiving messages between different components of the system.

mod interface;

use std::marker::PhantomData;

pub use interface::*;

mod message;
pub use message::*;

pub mod oneshot;
pub(crate) use oneshot::*;

mod envelope;
pub use envelope::*;

mod sends;
pub use sends::*;
use type_sets::AsTypeSet;

pub struct Set<T>(PhantomData<fn() -> T>);

impl<T: AsTypeSet> type_sets::AsTypeSet for Set<T> {
    type Set = T::Set;
}
