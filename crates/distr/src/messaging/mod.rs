//! Messages to actors on other nodes.
//!
//! A [`RemoteMessage`] is sent to a [`GlobalName`] through a [`RemoteAddress`];
//! the node that hosts the actor decodes it and delivers it like any local
//! message, and sends the reply back. A [`ClusterAddress`] is the same for an
//! actor that may be on this node too, which is then reached without leaving
//! the process.
//!
//! Which message types a node accepts is decided by registering them with
//! [`Cluster::register`]. Any registered message can then reach any local actor
//! that accepts it, addressed by its [`Name`](zestors_runtime::Name).
//!
//! Nothing here requires a message to be serde: it must be [`Encode`] and
//! [`Decode`], which every serde type is, and can be by hand for anything else.
//! [`Message`](zestors_interface::Message) itself is unchanged.
//!
//! A reply channel can be part of a message too: see [`RemoteRequest`].
//!
//! ```no_run
//! use serde::{Deserialize, Serialize};
//! use zestors::{
//!     distr::{ClusterAddress, ClusterNode, GlobalName},
//!     interface::{Envelope, Interface, Message},
//!     prelude::*,
//! };
//!
//! // A message that can cross the network: it has a stable id, and serde.
//! #[derive(Message, StableId, Serialize, Deserialize, Debug)]
//! #[msg(reply = u32, id = "1c3d5e7f-2a4b-4c6d-8e0f-a1b2c3d4e5f6")]
//! struct Double(u32);
//!
//! #[derive(Interface, Debug)]
//! enum CounterInterface {
//!     Double(Envelope<Double>),
//! }
//!
//! # async fn example(node: ClusterNode) -> Result<(), Box<dyn std::error::Error>> {
//! // On the node that runs the actor: accept the message from other nodes.
//! node.cluster().register::<Double>();
//!
//! // On another node: address the actor, and call it. This looks for the
//! // actor there, checking that it accepts the messages registered here.
//! let counter: ClusterAddress<CounterInterface> = node
//!     .cluster()
//!     .address(GlobalName::new("counter", "node-b"))
//!     .await?;
//! let doubled = counter.call(Double(21)).await?;
//! # Ok(())
//! # }
//! ```

mod address;
#[cfg(feature = "auto-register")]
mod auto_register;
mod codec;
mod dispatch;
mod error;
mod frame;
mod message;
mod node;
mod ops;
mod pending;
mod receive;
mod reply;
mod request;
mod send;
mod wire;

pub use address::{ClusterActorRef, ClusterAddress, LocalAddress, RemoteAddress, ClusterAddressRef};
#[cfg(feature = "auto-register")]
#[doc(hidden)]
pub use auto_register::{IfNot, IfRemote, Probe, Registration};
pub use codec::{Decode, DecodeError, Encode, EncodeError};
pub use error::{
    AddressError, CastFailure, RemoteCallError, RemoteCastError, RemoteError, RemoteOpError,
    RemoteReplyError,
};
pub use message::{RemoteMessage, RemoteSet};
pub use ops::{ClusterActorOps, RemoteInfo};
pub use reply::{RemoteReceipt, RemoteReply};
pub use request::RemoteRequest;

pub(crate) use node::CommunicationView;
use node::Started;
pub use send::{RemoteAccepts, RemoteCallOptions};
