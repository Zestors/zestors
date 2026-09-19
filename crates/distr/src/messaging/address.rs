//! What you hold to reach an actor: [`RemoteAddress`] for one on another node,
//! [`ClusterAddress`] for one that may be on this node too, and the
//! [`RemoteActorRef`] both are reached through.

use crate::{Cluster, GlobalName};
use std::{fmt, marker::PhantomData, time::Duration};
use zestors_runtime::{Address, Context, Dyn};

/// An actor on another node, that messages can be sent to: the remote analog of
/// [`Address`](zestors_runtime::Address).
///
/// Made with [`Cluster::address`]. `C` is what the actor is expected to accept,
/// either its [`Interface`](zestors_interface::Interface) or a set of messages
/// like `Dyn<(Ping, Double)>`, and only those can be sent. Whether the actor
/// really does is up to the node it runs on to say: a message it doesn't accept
/// is answered with [`RemoteError::NotAccepted`](super::RemoteError::NotAccepted).
///
/// Messages are sent with [`RemoteAccepts`](super::RemoteAccepts), and the actor
/// is operated on with [`RemoteActorOps`](super::RemoteActorOps). Messages sent to one actor arrive
/// in the order they were sent. A message that is not answered is not sent
/// again; delivery is at most once.
pub struct RemoteAddress<C: Context = Dyn> {
    pub(super) cluster: Cluster,
    pub(super) target: GlobalName,
    pub(super) timeout: Option<Duration>,
    _ctx: PhantomData<fn() -> C>,
}

impl<C: Context> Clone for RemoteAddress<C> {
    fn clone(&self) -> Self {
        Self {
            cluster: self.cluster.clone(),
            target: self.target.clone(),
            timeout: self.timeout,
            _ctx: PhantomData,
        }
    }
}

impl<C: Context> fmt::Debug for RemoteAddress<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteAddress")
            .field("target", &self.target)
            .finish()
    }
}

impl<C: Context> RemoteAddress<C> {
    pub(super) fn new(cluster: Cluster, target: GlobalName) -> Self {
        Self {
            cluster,
            target,
            timeout: None,
            _ctx: PhantomData,
        }
    }

    /// The actor this address is for.
    pub fn name(&self) -> &GlobalName {
        &self.target
    }

    /// How long [`RemoteAccepts::call`](super::RemoteAccepts::call) and [`RemoteReceipt::wait`](super::RemoteReceipt::wait) wait for a
    /// reply, instead of the node's
    /// [`ClusterConfig::call_timeout`](crate::ClusterConfig::call_timeout).
    /// [`RemoteCallOptions::timeout`] overrides it for one call.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// An actor somewhere in the cluster, that messages can be sent to and that can
/// be operated on: [`Local`](ClusterAddress::Local) if it is on this node, and
/// [`Remote`](ClusterAddress::Remote) if it is on another.
///
/// Made with [`Cluster::address`](crate::Cluster::address), or
/// from an [`Address`] or a [`RemoteAddress`]. Messages are sent with
/// [`RemoteAccepts`](super::RemoteAccepts), and the actor is operated on with
/// [`RemoteActorOps`](super::RemoteActorOps), exactly as with a
/// [`RemoteAddress`]: a local actor is simply reached without leaving the
/// process, and without the message being encoded.
///
/// That is all it offers: it isn't an [`ActorRef`](zestors_runtime::ActorRef), so
/// [`ActorOps`](zestors_runtime::ActorOps) isn't available on it, whichever it
/// is.
///
/// Delivery to a local actor differs from that to a remote one in a few ways:
/// - There is no default timeout to wait for a reply, as the node's
///   [`call_timeout`](crate::ClusterConfig::call_timeout) and
///   [`ClusterAddress::with_timeout`] are for remote actors. A
///   [`RemoteCallOptions::timeout`] is honoured.
/// - It works whether or not the node is running.
/// - The operations that need the node's messages registry
///   ([`members`](super::RemoteActorOps::members) and the others by [`Id`](crate::Id))
///   fail with [`RemoteOpError::Unsupported`](super::RemoteOpError::Unsupported).
pub enum ClusterAddress<C: Context = Dyn> {
    /// An actor on this node.
    Local(Address<C>),
    /// An actor on another node.
    Remote(RemoteAddress<C>),
}

impl<C: Context> ClusterAddress<C> {
    /// How long [`RemoteAccepts::call`](super::RemoteAccepts::call) waits for a
    /// reply from a remote actor, see [`RemoteAddress::with_timeout`]. It has no
    /// effect on a local actor.
    pub fn with_timeout(self, timeout: Duration) -> Self {
        match self {
            ClusterAddress::Local(address) => ClusterAddress::Local(address),
            ClusterAddress::Remote(address) => {
                ClusterAddress::Remote(address.with_timeout(timeout))
            }
        }
    }
}

impl<C: Context> RemoteActorRef for ClusterAddress<C> {
    type Ctx = C;

    fn route(&self) -> Route<'_, C> {
        match self {
            ClusterAddress::Local(address) => Route::Local(address),
            ClusterAddress::Remote(address) => Route::Remote(address),
        }
    }
}

impl<C: Context> Clone for ClusterAddress<C> {
    fn clone(&self) -> Self {
        match self {
            ClusterAddress::Local(address) => ClusterAddress::Local(address.clone()),
            ClusterAddress::Remote(address) => ClusterAddress::Remote(address.clone()),
        }
    }
}

impl<C: Context> fmt::Debug for ClusterAddress<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClusterAddress::Local(address) => f.debug_tuple("Local").field(address).finish(),
            ClusterAddress::Remote(address) => f.debug_tuple("Remote").field(address).finish(),
        }
    }
}

impl<C: Context> From<Address<C>> for ClusterAddress<C> {
    fn from(address: Address<C>) -> Self {
        ClusterAddress::Local(address)
    }
}

impl<C: Context> From<RemoteAddress<C>> for ClusterAddress<C> {
    fn from(address: RemoteAddress<C>) -> Self {
        ClusterAddress::Remote(address)
    }
}

/// How a [`RemoteActorRef`] reaches its actor. An implementation detail of
/// [`RemoteAccepts`](super::RemoteAccepts) and [`RemoteActorOps`].
#[doc(hidden)]
pub enum Route<'a, C: Context> {
    /// An actor on this node.
    Local(&'a Address<C>),
    /// An actor on another node.
    Remote(&'a RemoteAddress<C>),
}

/// A reference to an actor on another node, or on this one, which
/// [`RemoteAccepts`](super::RemoteAccepts) and [`RemoteActorOps`] work on:
/// [`RemoteAddress`] and [`ClusterAddress`](super::ClusterAddress).
///
/// Implement this trait, and both are automatically implemented for your type.
pub trait RemoteActorRef: Sync {
    /// The [`Context`] of the associated actor.
    type Ctx: Context;

    /// How the actor is reached.
    #[doc(hidden)]
    fn route(&self) -> Route<'_, Self::Ctx>;
}

impl<C: Context> RemoteActorRef for RemoteAddress<C> {
    type Ctx = C;

    fn route(&self) -> Route<'_, C> {
        Route::Remote(self)
    }
}
