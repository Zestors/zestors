//! Messages to actors in the cluster, local or remote.
//!
//! A [`ClusterAddress`] reaches an actor by its `GlobalName`. For an actor on
//! another node, `send` frames and sends the message, the other node's
//! `receive` decodes it through the handlers in `dispatch` and delivers it like
//! a local message, and the reply comes back through `pending`. For an actor on
//! this node, the message goes straight into its mailbox, unencoded.
//!
//! `Cluster`'s messaging half is in `node.rs`; its membership half is in
//! `cluster/state.rs`. The user-facing overview is the crate documentation.

mod address;
#[cfg(feature = "auto-register")]
mod auto_register;
mod codec;
mod dispatch;
mod error;
mod frame;
mod message;
mod monitors;
mod node;
mod ops;
mod pending;
mod receive;
mod reply;
mod request;
mod send;
mod wire;

pub use address::{ClusterActorRef, ClusterAddress};
#[cfg(feature = "auto-register")]
#[doc(hidden)]
pub use auto_register::{IfNot, IfRemote, Probe, Registration};
pub use codec::{Decode, DecodeError, Encode, EncodeError};
pub use error::{
    AddressError, CastFailure, ClusterCallError, ClusterCastError, ClusterOpError,
    ClusterReplyError, RemoteError,
};
pub use message::{RemoteMessage, RemoteSet};
pub use ops::{ActorInfo, ClusterActorOps};
pub use reply::{ClusterReceipt, ClusterReply};
pub use request::RemoteRequest;

#[doc(hidden)]
pub use dispatch::Handlers;
pub(crate) use node::CommunicationView;
use node::Started;
pub use send::{ClusterAccepts, ClusterCallOptions};
