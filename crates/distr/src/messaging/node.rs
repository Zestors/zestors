//! This node's side of messaging: what it accepts from other nodes, the
//! addresses it makes, and the state it has while it runs.

use super::{
    AddressError, ClusterActorOps as _, ClusterAddress, ClusterOpError, ClusterReplyError,
    RemoteError, RemoteSet, dispatch::Handlers, monitors::Monitors, pending::Pending, receive,
};
use crate::{
    Cluster, GlobalName, MessageId,
    link::{Links, Protocol},
};
use std::{
    sync::{Arc, RwLock, atomic::AtomicU64},
    time::Duration,
};
use tokio_util::sync::{CancellationToken, DropGuard};
use type_sets::{AsTypeSet, Members};
use zestors_interface::Interface;
use zestors_runtime::{Address, Context, Dyn, Registry};

/// What a node needs to message actors on other nodes, kept inside a
/// [`Cluster`].
pub(crate) struct CommunicationView {
    pub(super) call_timeout: Duration,
    /// How many lanes to a peer messages between actors are spread over.
    pub(super) shards: u8,
    /// The messages this node accepts, fixed when it was built.
    pub(super) handlers: Handlers,
    /// Set while the node runs.
    pub(super) running: RwLock<Option<Started>>,
    /// Numbers the answers this node is waiting for. Both a call and a request
    /// handed to another node inside a message draw from it, because both are
    /// answered by a [`Frame::Reply`](super::frame::Frame::Reply) and looked up
    /// in the one [`Pending`] table: two counters would collide there.
    pub(super) next_call: AtomicU64,
    /// The monitors other nodes hold on actors here.
    pub(super) monitors: Monitors,
}

/// What is there once the node runs.
#[derive(Clone)]
pub(super) struct Started {
    pub(super) links: Links,
    pub(super) pending: Arc<Pending>,
}

impl CommunicationView {
    pub(crate) fn new(call_timeout: Duration, shards: u8, handlers: Handlers) -> Self {
        Self {
            call_timeout,
            shards,
            handlers,
            running: RwLock::new(None),
            next_call: AtomicU64::new(0),
            monitors: Monitors::default(),
        }
    }

    pub(super) fn running(&self) -> Option<Started> {
        self.running.read().expect("Not poisoned").clone()
    }
}

/// Messaging between actors: making [`ClusterAddress`]es to send with.
impl Cluster {
    /// How many monitors other nodes are holding on actors here. For tests that
    /// check a monitor is let go of.
    #[doc(hidden)]
    pub fn monitors_held(&self) -> usize {
        self.messaging().monitors.len()
    }

    /// The ids `address` accepts, among the messages this node was built with,
    /// sorted. The one answer to that question, so that a node says the same
    /// about an actor whether it is asked from here or over the network.
    pub(crate) fn accepted_ids(&self, address: &Address) -> Vec<MessageId> {
        self.messaging().handlers.accepted_ids(address)
    }

    /// Whether `address` accepts every message in `ids`, without the list
    /// itself going anywhere.
    pub(crate) fn accepts_ids(&self, address: &Address, ids: &[MessageId]) -> bool {
        self.messaging().handlers.accepts_ids(address, ids)
    }

    /// An actor on this node as a [`ClusterAddress`], so that it can be
    /// handled the same way as one anywhere else in the cluster.
    pub fn local_address<C: Context>(&self, address: Address<C>) -> ClusterAddress<C> {
        ClusterAddress::local(self.clone(), address)
    }

    /// An address for the actor `target`, which accepts the interface `I`,
    /// wherever it is in the cluster.
    ///
    /// The actor is looked up where it is: in this node's registry, or by
    /// asking the node it is on, which has to be a reachable member. It has to
    /// be registered under that name, and accept every message in `I`.
    ///
    /// Every message in `I` has to be a [`RemoteMessage`](crate::RemoteMessage)
    /// (see [`RemoteSet`]). For an interface with messages that can't cross the
    /// network, address the part that can with [`Cluster::address_dyn`].
    ///
    /// A remote address is resolved by name on every send, so it follows the
    /// actor across restarts under the same name.
    ///
    /// ```no_run
    /// # use zestors::{distr::{Cluster, GlobalName}, interface::{Envelope, Interface, Message}, prelude::*};
    /// # use serde::{Deserialize, Serialize};
    /// # #[derive(Message, StableId, Serialize, Deserialize, Debug)]
    /// # #[msg(reply = u32, id = "1c3d5e7f-2a4b-4c6d-8e0f-a1b2c3d4e5f7")]
    /// # struct Double(u32);
    /// # #[derive(Interface, Debug)]
    /// # enum CalcInterface { Double(Envelope<Double>) }
    /// # async fn example(cluster: Cluster) -> Result<(), Box<dyn std::error::Error>> {
    /// cluster.wait_for_members(1).await;
    /// let calc = cluster
    ///     .address::<CalcInterface>(GlobalName::new("calc", "node-b"))
    ///     .await?;
    /// assert_eq!(calc.call(Double(21)).await?, 42);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn address<I: Interface>(
        &self,
        target: GlobalName,
    ) -> Result<ClusterAddress<I>, AddressError>
    where
        I::Set: RemoteSet,
    {
        if target.node() == self.name() {
            let address = Registry::local().get_typed::<I>(target.name())?;
            return Ok(self.local_address(address));
        }
        self.resolve(target, <I::Set as RemoteSet>::REMOTE_IDS)
            .await
    }

    /// Like [`Cluster::address`], for the actor to accept a set of messages like
    /// `Dyn<(Ping, Double)>`.
    pub async fn address_dyn<S>(
        &self,
        target: GlobalName,
    ) -> Result<ClusterAddress<Dyn<S>>, AddressError>
    where
        S: AsTypeSet + Members + RemoteSet + 'static,
    {
        if target.node() == self.name() {
            let address = Registry::local().get_dyn::<S>(target.name())?;
            return Ok(self.local_address(address));
        }
        self.resolve(target, S::REMOTE_IDS).await
    }

    /// Asks the node `target` is on whether the actor is there, and accepts
    /// the messages with these ids.
    async fn resolve<C: Context>(
        &self,
        target: GlobalName,
        ids: &'static [MessageId],
    ) -> Result<ClusterAddress<C>, AddressError> {
        let name = target.name().clone();
        let address = ClusterAddress::remote(self.clone(), target);
        match address.is_superset_of(ids).await {
            Ok(true) => Ok(address),
            Ok(false) => Err(AddressError::TypeMismatch(name)),
            Err(ClusterOpError::Reply(ClusterReplyError::Remote(RemoteError::NoSuchActor))) => {
                Err(AddressError::NoSuchActor(name))
            }
            Err(error) => Err(AddressError::Remote(error)),
        }
    }

    /// Starts taking in messages, and sends what is asked to, until the returned
    /// [`Serving`] is stopped.
    pub(crate) fn start(&self, links: Links) -> Serving {
        let running = Started {
            links: links.clone(),
            pending: Arc::new(Pending::default()),
        };
        let token = CancellationToken::new();
        let serving = receive::serve(
            self.clone(),
            running.clone(),
            links.subscribe(Protocol::ACTORS),
            links.peer_events(),
            self.subscribe(),
        );
        tokio::spawn(token.clone().run_until_cancelled_owned(async move {
            serving.await;
        }));
        *self.messaging().running.write().expect("Not poisoned") = Some(running);
        Serving {
            cluster: self.clone(),
            _stop: token.drop_guard(),
        }
    }
}

/// Messaging while the node runs. Stops when dropped, however that happens:
/// the node exiting on its own, or its task being cancelled part way.
pub(crate) struct Serving {
    cluster: Cluster,
    _stop: DropGuard,
}

impl Drop for Serving {
    /// Stops taking in messages, and gives up on the calls still waiting.
    ///
    /// This is in `Drop` and not a method that the node remembers to call, so
    /// that a node which stops without getting that far still tells whoever is
    /// waiting, rather than leaving them to wait out the call timeout.
    fn drop(&mut self) {
        if let Some(running) = self
            .cluster
            .messaging()
            .running
            .write()
            .expect("Not poisoned")
            .take()
        {
            running.pending.fail_all();
        }
    }
}
