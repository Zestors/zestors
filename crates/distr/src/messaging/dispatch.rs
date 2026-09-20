//! Delivers a message to a local actor, knowing its type.

use super::{Encode, RemoteError, RemoteMessage, ops, wire::Wire};
use crate::{Cluster, MessageId};
use bytes::Bytes;
use indexmap::IndexMap;
use std::{any::TypeId, future::Future, marker::PhantomData, pin::Pin, sync::Arc};
use zestors_interface::Receipt;
use zestors_runtime::{ActorRef, Address, errors::CastDynError, prelude::*};

/// The messages a node accepts, and what delivers each of them.
///
/// Fixed once the node is built — messages are registered on
/// [`ClusterConfig`](crate::ClusterConfig), never while the node runs — so this
/// needs no locking, and the whole node shares one of them.
#[doc(hidden)]
pub struct Handlers {
    by_id: IndexMap<MessageId, Arc<dyn Handler>>,
    /// The id of each registered message, by its Rust type. Only a way into
    /// `by_id`, so that a question asked about an actor's [`TypeId`]s doesn't
    /// have to walk every handler.
    by_type: IndexMap<TypeId, MessageId>,
}

impl Handlers {
    /// The operations every node handles, and nothing else yet.
    pub(crate) fn new() -> Self {
        let mut handlers = Self {
            by_id: IndexMap::new(),
            by_type: IndexMap::new(),
        };
        ops::register(&mut handlers.by_id);
        handlers
    }

    /// Makes `M` deliverable to actors on this node. Registering it twice is
    /// the same as once.
    pub(crate) fn insert<M: RemoteMessage>(&mut self) {
        self.by_id.insert(M::Id, Arc::new(Typed::<M>(PhantomData)));
        self.by_type.insert(TypeId::of::<M>(), M::Id);
    }

    /// What delivers the message `id`, if this node accepts it at all.
    pub(super) fn get(&self, id: &MessageId) -> Option<&Arc<dyn Handler>> {
        self.by_id.get(id)
    }

    /// Whether `address` accepts the message `id`, in one lookup.
    pub(crate) fn accepts_id(&self, address: &Address, id: MessageId) -> bool {
        self.by_id
            .get(&id)
            .is_some_and(|handler| handler.accepts(address))
    }

    /// The ids `address` accepts, among the messages registered here, sorted.
    ///
    /// Walks the actor's own message types rather than every handler: an actor
    /// accepts few messages, a node may know many. The built-in operations are
    /// not listed, since they are for the channel and not the mailbox.
    pub(crate) fn accepted_ids(&self, address: &impl ActorRef) -> Vec<MessageId> {
        let members = address.members();
        let mut accepts: Vec<MessageId> = Vec::with_capacity(members.len());
        accepts.extend(
            members
                .iter()
                .filter_map(|type_id| self.by_type.get(type_id).copied()),
        );
        accepts.sort_unstable();
        accepts
    }
}

impl Default for Handlers {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// What is left to do for a message once it has been delivered: wait for the
/// actor's reply, and encode it.
pub(super) type ReplyFuture = BoxFuture<Result<Bytes, RemoteError>>;

/// Delivers messages of one type to local actors. One is registered per
/// message type; it is what knows the type, so that everything else needn't.
pub(super) trait Handler: Send + Sync {
    /// Decodes a message and puts it in the actor's mailbox, waiting out its
    /// backpressure. If `reply` is set, the result is what to wait for then.
    ///
    /// `wire` is the node the message is from.
    fn deliver(
        &self,
        wire: Wire,
        address: Address,
        payload: Bytes,
        reply: bool,
    ) -> BoxFuture<Result<Option<ReplyFuture>, RemoteError>>;

    /// Whether `address` accepts messages of this type.
    fn accepts(&self, address: &Address) -> bool;

    /// Whether the messages are for the actor itself rather than its mailbox,
    /// so that they needn't wait for the messages before them.
    fn bypasses_queue(&self) -> bool {
        false
    }
}

pub(super) struct Typed<M>(pub(super) PhantomData<fn() -> M>);

/// An operation on an actor, see [`super::ops`]: handled by the node, not by
/// the actor's mailbox.
pub(super) trait Operation: RemoteMessage + Send + 'static {
    /// Carries the operation out on `address`.
    fn run(
        self,
        address: Address,
        cluster: Cluster,
    ) -> BoxFuture<Result<Self::Output, RemoteError>>;
}

/// Delivers an [`Operation`] of one type.
pub(super) struct Builtin<M>(pub(super) PhantomData<fn() -> M>);

impl<M: Operation> Handler for Builtin<M> {
    fn deliver(
        &self,
        wire: Wire,
        address: Address,
        payload: Bytes,
        reply: bool,
    ) -> BoxFuture<Result<Option<ReplyFuture>, RemoteError>> {
        Box::pin(async move {
            let msg = wire
                .scope(|| M::decode(payload))
                .map_err(|error| RemoteError::Decode(error.to_string()))?;
            let running = msg.run(address, wire.session().cluster().clone());
            if !reply {
                tokio::spawn(async move {
                    let _ = running.await;
                });
                return Ok(None);
            }
            let reply: ReplyFuture = Box::pin(async move {
                running
                    .await?
                    .encode()
                    .map_err(|error| RemoteError::Encode(error.to_string()))
            });
            Ok(Some(reply))
        })
    }

    fn accepts(&self, _: &Address) -> bool {
        false
    }

    fn bypasses_queue(&self) -> bool {
        true
    }
}

impl<M: RemoteMessage> Handler for Typed<M> {
    fn deliver(
        &self,
        wire: Wire,
        address: Address,
        payload: Bytes,
        reply: bool,
    ) -> BoxFuture<Result<Option<ReplyFuture>, RemoteError>> {
        Box::pin(async move {
            let msg = wire
                .scope(|| M::decode(payload))
                .map_err(|error| RemoteError::Decode(error.to_string()))?;
            let receipt = match address.cast_dyn(msg).await {
                Ok(receipt) => receipt,
                Err(CastDynError::Closed(_)) => return Err(RemoteError::Closed),
                Err(CastDynError::NotAccepted(_)) => return Err(RemoteError::NotAccepted),
            };
            if !reply {
                return Ok(None);
            }
            let reply: ReplyFuture = Box::pin(async move {
                let output = receipt.wait().await.map_err(|_| RemoteError::NoReply)?;
                output
                    .encode()
                    .map_err(|error| RemoteError::Encode(error.to_string()))
            });
            Ok(Some(reply))
        })
    }

    fn accepts(&self, address: &Address) -> bool {
        address.accepts::<M>()
    }
}
