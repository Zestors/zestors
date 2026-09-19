//! The seam between the membership layer and the network.
//!
//! [`Net`] is everything the membership protocol needs from a transport: hand
//! it a frame for a peer, and tell it when a peer is gone. What comes back is
//! the stream of [`Event`]s handed out when the transport is created. The real
//! implementation is [`Transport`](crate::quic::Transport) over QUIC; tests use
//! an in-memory fabric, see `sim`.

use crate::{Member, NodeId};
use bytes::Bytes;

/// Carries [`Frame`]s to other nodes on behalf of the membership layer.
pub(crate) trait Net: Send + Sync + 'static {
    /// Queues `frame` for delivery to `to`.
    ///
    /// Never blocks. Delivery is best-effort: the message is dropped if the
    /// peer is unreachable or its queue is full.
    fn send(&self, to: &Member, frame: Frame);

    /// Stops talking to `node` if it is in that generation or an older one, so
    /// the next message starts afresh. Called when the node has been declared
    /// down or has moved.
    fn forget(&self, node: &NodeId, generation: u64);
}

/// A message between two nodes.
#[derive(Debug, Clone)]
pub(crate) enum Frame {
    /// Opaque membership protocol bytes.
    Gossip(Bytes),
    /// The sender is shutting down cleanly and is not going to come back.
    Departure,
}

/// Something the transport reports to the membership layer.
#[derive(Debug)]
pub(crate) enum Event {
    /// A message from a peer.
    Received(Incoming),
    /// Repeated attempts to connect to the peer have failed. It is retried with
    /// growing pauses in between until it answers.
    Unreachable(NodeId),
    /// The peer can be connected to again, after having been reported unreachable.
    Reachable(NodeId),
}

/// A [`Frame`] received from a peer whose identity was established when the
/// connection was set up.
#[derive(Debug)]
pub(crate) struct Incoming {
    pub(crate) from: NodeId,
    pub(crate) generation: u64,
    pub(crate) frame: Frame,
}

