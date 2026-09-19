//! [`ClusterAddress`]: an actor that is on this node or on another, addressed
//! the same way.

use super::{
    RemoteActorRef, RemoteAddress, RemoteCallOptions, RemoteCastError, RemoteMessage,
    actor_ops::Route,
};
use std::{fmt, time::Duration};
use zestors_runtime::{
    ActorOps as _, Address, CallOptions, Context, Dyn,
    errors::{CastDynError, TryCastDynError},
};

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

/// Sends `msg` to an actor on this node, waiting for room in its mailbox.
pub(super) async fn cast<M: RemoteMessage, C: Context>(
    address: &Address<C>,
    msg: M,
    options: RemoteCallOptions,
) -> Result<M::RemoteReceipt, RemoteCastError<M>> {
    // Not `Accepts::cast`, which panics if a `Dyn` address is for an actor that
    // doesn't accept the message.
    match address.cast_dyn_with(msg, CallOptions::default()).await {
        Ok(receipt) => Ok(M::local_receipt(receipt, options.timeout)),
        Err(CastDynError::Closed(msg)) => Err(RemoteCastError::Closed(msg)),
        Err(CastDynError::NotAccepted(msg)) => Err(RemoteCastError::NotAccepted(msg)),
    }
}

/// Like [`cast`], but fails if the mailbox is full.
pub(super) fn try_cast<M: RemoteMessage, C: Context>(
    address: &Address<C>,
    msg: M,
    options: RemoteCallOptions,
) -> Result<M::RemoteReceipt, RemoteCastError<M>> {
    match address.try_cast_dyn_with(msg, CallOptions::default()) {
        Ok(receipt) => Ok(M::local_receipt(receipt, options.timeout)),
        Err(TryCastDynError::Closed(msg)) => Err(RemoteCastError::Closed(msg)),
        Err(TryCastDynError::Full(msg)) => Err(RemoteCastError::Full(msg)),
        Err(TryCastDynError::NotAccepted(msg)) => Err(RemoteCastError::NotAccepted(msg)),
    }
}
