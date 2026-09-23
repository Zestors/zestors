//! The message vocabulary of `zestors`: what an actor accepts, and how it
//! replies.
//!
//! - [`Message`] is implemented by every message type, and is usually
//!   [derived](derive@Message). Its [`Kind`](Message::Kind) says whether it
//!   expects a reply: [`Cast`] for fire-and-forget, [`Call`] for
//!   request/reply, with the reply's type as [`Output`](Message::Output).
//! - Sending a message packs it into an [`Envelope`] together with a
//!   [`Resolver`], which the receiving actor answers with. The sender keeps the
//!   matching [`Receipt`] and waits on it. For a `Call` message these are a
//!   [`Request<T>`] and a [`Reply<T>`]; for a `Cast` message both are `()`.
//! - [`Interface`] is the set of messages an actor accepts: an enum with one
//!   [`Envelope`] variant per message, usually [derived](derive@Interface).
//!
//! This crate only defines the contract; delivering messages is up to
//! `zestors-runtime`. The [zestors book](https://zestors.github.io/zestors/messages.html)
//! explains messages and interfaces in more depth. To send a message to
//! another node, it also needs a `StableId`; see `zestors-distr`.
//!
//! # Example
//!
//! What sending a request amounts to, without an actor: the receiving side
//! answers the [`Request`], and the sending side gets the answer from the
//! [`Reply`].
//!
//! ```
//! # use zestors::interface::{Envelope, Message, Receipt as _, Resolver as _};
//! #[derive(Message, Debug)]
//! #[msg(reply = u32)]
//! #[zestors(interface_path = "zestors::interface")]
//! struct DoubleMe(u32);
//!
//! # #[tokio::main]
//! # async fn main() {
//! let (envelope, receipt) = Envelope::new_pair(DoubleMe(21));
//!
//! // The receiving side unpacks the envelope and resolves the request.
//! let Envelope { msg: DoubleMe(n), req: resolver } = envelope;
//! resolver.resolve(n * 2).unwrap();
//!
//! // The sending side waits on the receipt for the reply.
//! assert_eq!(receipt.wait().await.unwrap(), 42);
//! # }
//! ```

#[doc(hidden)]
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
