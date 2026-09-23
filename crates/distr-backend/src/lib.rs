//! How nodes reach each other.
//!
//! A [`Backend`] is the network a cluster runs on. It is deliberately small:
//! it connects to and accepts connections from other nodes, and a connection
//! offers ordered, reliable, bidirectional streams and unreliable datagrams.
//! Everything else is done for it by the cluster, the same for every backend:
//! keeping one connection per peer, reconnecting with backoff, telling
//! unreachable from down, ordering and framing messages, and routing them to
//! the layer they are for.
//!
//! A cluster runs over any implementation of [`Backend`], [`Endpoint`] and
//! [`Connection`], handed to `ClusterConfig::new` in `zestors-distr`. The QUIC
//! backend is in the `zestors-distr-quic` crate.
//!
//! # What a backend must provide
//!
//! - **Verified identity.** [`Connection::peer`] is the node on the other end,
//!   established by the backend and not taken from the peer's word: from a
//!   certificate, a key, or credentials. [`Endpoint::connect`] reaches the node
//!   with the given name at the given address, and fails if it isn't that
//!   node; [`Endpoint::accept`] derives the name of whoever connected. The
//!   cluster trusts it completely, since without it any node could
//!   impersonate another.
//! - **Streams.** Bytes written to a stream arrive complete and in order, or
//!   not at all once the connection is lost. Streams are independent: a stall
//!   on one must not hold up the others, which is what a backend with native
//!   streams gets for free, and one without has to arrange.
//! - **Datagrams**, if it has them. A backend without returns
//!   [`DatagramError::Unsupported`], and the cluster uses streams instead.

mod node_addr;
mod node_name;

pub use node_addr::NodeAddr;
pub use node_name::NodeName;

use bytes::Bytes;
use std::{future::Future, io, time::Duration};
use tokio::io::{AsyncRead, AsyncWrite};

/// The sending half of a stream. Shutting it down ends the stream, after
/// everything written to it has been sent.
pub type SendStream = Box<dyn AsyncWrite + Send + Unpin>;

/// The receiving half of a stream. Reads to the end once the peer has shut
/// down its half.
pub type RecvStream = Box<dyn AsyncRead + Send + Unpin>;

/// A way of connecting nodes, see the [module docs](self).
pub trait Backend: Send + 'static {
    /// What the backend becomes once started: this node on the network.
    type Endpoint: Endpoint;

    /// Brings up the network for the node described by `local`.
    fn start(
        self,
        local: NodeIncarnation,
    ) -> impl Future<Output = io::Result<Self::Endpoint>> + Send;
}

/// The node a [`Backend`] is started for.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct NodeIncarnation {
    /// The node's name.
    pub name: NodeName,
    /// Which incarnation of the node this is: higher than that of any earlier
    /// run of the same node.
    pub generation: u64,
}

impl NodeIncarnation {
    /// Describes the node `name` in its incarnation `generation`.
    pub fn new(name: NodeName, generation: u64) -> Self {
        Self { name, generation }
    }
}

/// A node's presence on the network: it connects to peers and accepts theirs.
pub trait Endpoint: Send + Sync + 'static {
    /// A connection to one peer.
    type Connection: Connection;

    /// The address other nodes reach this one on. The node's configuration can
    /// override it, for example behind NAT.
    fn local_addr(&self) -> io::Result<NodeAddr>;

    /// Connects to the node `node` at `addr`. Fails if what answers there is
    /// not that node: the connection's [`peer`](Connection::peer) is `node`.
    fn connect(
        &self,
        addr: &NodeAddr,
        node: &NodeName,
    ) -> impl Future<Output = io::Result<Self::Connection>> + Send;

    /// The next connection a peer has opened to this node, once it is ready to
    /// be used and the peer's identity is established. Fails once the endpoint
    /// is closed. Connections that fail to be set up, or whose peer can't be
    /// identified, are not reported.
    fn accept(&self) -> impl Future<Output = io::Result<Self::Connection>> + Send;

    /// Closes the endpoint and every connection on it, waiting at most `grace`
    /// for what is in flight.
    fn close(&self, grace: Duration) -> impl Future<Output = ()> + Send;
}

/// A connection to one peer.
pub trait Connection: Send + Sync + 'static {
    /// Opens a stream to the peer. It becomes known to the peer through
    /// [`Connection::accept_stream`] once something is written to it.
    fn open_stream(&self) -> impl Future<Output = io::Result<(SendStream, RecvStream)>> + Send;

    /// The next stream the peer has opened. Fails once the connection is closed.
    fn accept_stream(&self) -> impl Future<Output = io::Result<(SendStream, RecvStream)>> + Send;

    /// Sends `data` unreliably: it may be lost, or arrive out of order. Never
    /// waits.
    fn send_datagram(&self, data: Bytes) -> Result<(), DatagramError>;

    /// The next datagram from the peer. Fails once the connection is closed.
    fn recv_datagram(&self) -> impl Future<Output = io::Result<Bytes>> + Send;

    /// The node on the other end, as verified by the backend.
    fn peer(&self) -> &NodeName;

    /// Whether the connection is closed, by either side or by an error.
    fn is_closed(&self) -> bool;

    /// Closes the connection.
    fn close(&self);
}

/// Why a datagram couldn't be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DatagramError {
    /// It doesn't fit in one datagram.
    TooLarge,
    /// The backend or the peer doesn't do datagrams.
    Unsupported,
    /// The connection is closed.
    Closed,
}
