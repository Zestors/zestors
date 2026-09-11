//! Defines the core interface/messaging traits for zestors
//!
//! There are two big traits defined in this crate:
//! - [`Message`] specifies what kind of reply a message should return.
//! - [`Interface`] specifies the messages that an actor can accept.
//!
//! Messages are sent inside an [`Envelope`], containing both the message payload,
//! and the associated [`Resolver`].

pub mod prelude {
    pub use crate::{
        Envelope, Interface, Message,
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

pub use zestors_codegen::{Interface, Message};
