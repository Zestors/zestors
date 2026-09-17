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
//!
//! This crate only defines the message *contract* - it has no notion of an
//! actor or a channel to deliver a message through. That's the job of
//! `zestors-runtime`, which sends a [`Message`] by constructing an
//! [`Envelope`] for it and pushing that onto an actor's queue; see its
//! `Accepts::cast`/`Accepts::call` for the sending side.
//!
//! # Example
//!
//! A request-style message: it derives [`Message`] with a `reply` type, so
//! its [`Resolver`] is a [`Request<T>`] and its [`Receipt`] is a
//! [`Reply<T>`]. Below, `resolver`/`receipt` stand in for what a real
//! channel implementation would keep on opposite ends of an [`Envelope`]:
//! the receiving side resolves it, the sending side awaits the result.
//!
//! ```
//! # use zestors::interface::{Envelope, Message, Receipt as _, Resolver as _};
//!
//! #[derive(Message, Debug)]
//! #[msg(reply = u32)]
//! #[msg(path = "zestors::interface")]
//! struct DoubleMe(u32);
//!
//! # #[tokio::main]
//! # async fn main() {
//! // Pair the message with a fresh resolver/receipt, exactly like sending
//! // it through a channel would.
//! let (envelope, receipt) = Envelope::new_pair(DoubleMe(21));
//!
//! // The "receiving" side unpacks the envelope, computes a reply, and
//! // resolves it.
//! let Envelope { msg: DoubleMe(n), req: resolver } = envelope;
//! resolver.resolve(n * 2).unwrap();
//!
//! // The "sending" side awaits the receipt to get that reply back.
//! assert_eq!(receipt.wait().await.unwrap(), 42);
//! # }
//! ```

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
