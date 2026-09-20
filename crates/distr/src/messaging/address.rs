//! What you hold to reach an actor anywhere in the cluster: [`ClusterAddress`],
//! and the [`ClusterActorRef`] it is reached through.

use crate::{Cluster, GlobalName, NodeName};
use std::{fmt, time::Duration};
use zestors_runtime::{Address, Context, Dyn};

/// An actor somewhere in the cluster, that messages can be sent to and that can
/// be operated on — on this node or on another.
///
/// Made with [`Cluster::address`](crate::Cluster::address),
/// [`Cluster::address_dyn`](crate::Cluster::address_dyn) or
/// [`Cluster::local_address`](crate::Cluster::local_address). `C` is what the
/// actor is expected to accept, either its
/// [`Interface`](zestors_interface::Interface) or a set of messages like
/// `Dyn<(Ping, Double)>`, and only those can be sent. For an actor on another
/// node, whether it really does accept them is up to the node it runs on to
/// say: a message it doesn't accept is answered with
/// [`RemoteError::NotAccepted`](super::RemoteError::NotAccepted).
///
/// Messages are sent with [`ClusterAccepts`](super::ClusterAccepts), and the
/// actor is operated on with [`ClusterActorOps`](super::ClusterActorOps),
/// wherever it is: an actor on this node is simply reached without leaving the
/// process, and without the message being encoded. Messages sent to one actor
/// arrive in the order they were sent. A message that is not answered is not
/// sent again; delivery is at most once.
///
/// That is all it offers: it isn't an [`ActorRef`](zestors_runtime::ActorRef),
/// so [`ActorOps`](zestors_runtime::ActorOps) isn't available on it. For an
/// actor on this node, [`ClusterAddress::local_address`] gives the plain
/// [`Address`], which is.
///
/// Delivery to an actor on this node still differs from that to one on another:
/// - There is no default timeout to wait for a reply, as the node's
///   [`call_timeout`](crate::ClusterConfig::call_timeout) and
///   [`ClusterAddress::with_timeout`] are for actors on other nodes. A
///   [`ClusterCallOptions::timeout`](super::ClusterCallOptions::timeout) is honoured.
/// - It works whether or not the node is running.
pub struct ClusterAddress<C: Context = Dyn> {
    cluster: Cluster,
    target: Target<C>,
}

/// Where the actor is, and what it takes to reach it there.
///
/// Private to the crate and never re-exported: which side of the network an
/// actor is on is what a [`ClusterAddress`] exists to absorb, not something
/// its holder chooses between.
pub(super) enum Target<C: Context> {
    /// An actor on this node, reached through its own address.
    Local(Address<C>),
    /// An actor on another node, reached by name over the network.
    Remote {
        name: GlobalName,
        /// How long to wait for a reply, if not the node's
        /// [`call_timeout`](crate::ClusterConfig::call_timeout). Only the
        /// remote half has one: it is the network crossing that is waited on.
        timeout: Option<Duration>,
    },
}

impl<C: Context> ClusterAddress<C> {
    /// An actor on this node.
    pub(super) fn local(cluster: Cluster, address: Address<C>) -> Self {
        Self {
            cluster,
            target: Target::Local(address),
        }
    }

    /// An actor on another node. Whether it is really there, and really
    /// accepts `C`, is asked separately — see [`Cluster::address`](crate::Cluster::address).
    pub(super) fn remote(cluster: Cluster, name: GlobalName) -> Self {
        Self {
            cluster,
            target: Target::Remote {
                name,
                timeout: None,
            },
        }
    }

    /// Where the actor is: what every operation branches on.
    pub(super) fn target(&self) -> &Target<C> {
        &self.target
    }

    /// The cluster the actor is reached through. Needed for an actor on this
    /// node as much as for one on another: the operations that go by
    /// [`MessageId`](crate::MessageId) — such as
    /// [`members`](super::ClusterActorOps::members) — are answered from the
    /// same registry of [registered](crate::ClusterConfig::register) messages
    /// either way, so that a node says the same about an actor however it is
    /// asked.
    pub(super) fn cluster(&self) -> &Cluster {
        &self.cluster
    }

    /// How long [`ClusterAccepts::call`](super::ClusterAccepts::call) and
    /// [`ClusterReceipt::wait`](super::ClusterReceipt::wait) wait for a reply
    /// from an actor on another node, instead of the node's
    /// [`ClusterConfig::call_timeout`](crate::ClusterConfig::call_timeout).
    /// [`ClusterCallOptions::timeout`](super::ClusterCallOptions::timeout)
    /// overrides it for one call.
    ///
    /// It has no effect on an actor on this node, which is not waited on
    /// across a network and has no default timeout to replace.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        if let Target::Remote {
            timeout: current, ..
        } = &mut self.target
        {
            *current = Some(timeout);
        }
        self
    }

    /// Whether the actor is on this node, and so is reached without the
    /// network.
    pub fn is_local(&self) -> bool {
        matches!(self.target, Target::Local(_))
    }

    /// Whether the actor is on another node.
    pub fn is_remote(&self) -> bool {
        !self.is_local()
    }

    /// The node the actor is on.
    pub fn node(&self) -> &NodeName {
        match &self.target {
            Target::Local(_) => self.cluster.name(),
            Target::Remote { name, .. } => name.node(),
        }
    }

    /// The actor's address on this node, which
    /// [`ActorOps`](zestors_runtime::ActorOps) works on, or `None` if the actor
    /// is on another node.
    pub fn local_address(&self) -> Option<&Address<C>> {
        match &self.target {
            Target::Local(address) => Some(address),
            Target::Remote { .. } => None,
        }
    }

    /// Takes the actor's address on this node out, see
    /// [`ClusterAddress::local_address`].
    pub fn into_local_address(self) -> Option<Address<C>> {
        match self.target {
            Target::Local(address) => Some(address),
            Target::Remote { .. } => None,
        }
    }
}

impl<C: Context> ClusterActorRef for ClusterAddress<C> {
    type Ctx = C;

    fn cluster_address(&self) -> &ClusterAddress<C> {
        self
    }
}

impl<C: Context> Clone for ClusterAddress<C> {
    fn clone(&self) -> Self {
        Self {
            cluster: self.cluster.clone(),
            target: match &self.target {
                Target::Local(address) => Target::Local(address.clone()),
                Target::Remote { name, timeout } => Target::Remote {
                    name: name.clone(),
                    timeout: *timeout,
                },
            },
        }
    }
}

impl<C: Context> fmt::Debug for ClusterAddress<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("ClusterAddress");
        match &self.target {
            Target::Local(address) => debug.field("local", address),
            Target::Remote { name, .. } => debug.field("remote", name),
        }
        .finish()
    }
}

/// A reference to an actor anywhere in the cluster, which
/// [`ClusterAccepts`](super::ClusterAccepts) and
/// [`ClusterActorOps`](super::ClusterActorOps) work on.
///
/// Implement it for a type of yours that holds a [`ClusterAddress`], and both
/// of those come with it.
pub trait ClusterActorRef: Sync {
    /// The [`Context`] of the associated actor.
    type Ctx: Context;

    /// The actor this refers to.
    fn cluster_address(&self) -> &ClusterAddress<Self::Ctx>;
}
