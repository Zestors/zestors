//! Defines the core interface/messaging traits for `zestors`.
//!
//! Sending a message packages it into an [`Envelope`], pairing the message
//! payload with a [`Resolver`] that the receiving actor uses to send back a
//! reply. The sender is immediately given the matching [`Receipt`], which
//! resolves once that reply arrives.
//!
//! - [`Message`] specifies a message type's [`Receipt`]/[`Resolver`]/output,
//!   and is usually [derived](derive@Message) rather than implemented by hand.
//! - [`Receipt`]/[`Resolver`] come in two forms: `()`/`()` for
//!   fire-and-forget messages that expect no reply, and [`Reply<T>`]/
//!   [`Request<T>`] for messages that expect a reply of type `T`.
//! - [`Interface`] specifies the set of message types an actor accepts, and
//!   how to convert between a concrete message and a type-erased
//!   [`AnyEnvelope`] for dynamic sending; like [`Message`], it is usually
//!   [derived](derive@Interface).

pub mod prelude {
    pub use crate::{
        Envelope, Interface, Message,
        oneshot::{Reply, Request},
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

pub use zestors_codegen::{Interface, Message};
