//! Delivers a message to a local actor, knowing its type.

use super::{Encode, RemoteError, RemoteMessage, ops, wire::Wire};
use crate::{Cluster, MessageId, NodeName};
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
    by_id: IndexMap<MessageId, Registered>,
    /// The id of each registered message, by its Rust type. Only a way into
    /// `by_id`, so that a question asked about an actor's [`TypeId`]s doesn't
    /// have to walk every handler.
    by_type: IndexMap<TypeId, MessageId>,
}

/// A handler, and the type it was registered for: enough to tell one message
/// registered twice from two messages that claim the same [`MessageId`].
struct Registered {
    handler: Arc<dyn Handler>,
    type_id: TypeId,
    type_name: &'static str,
}

impl Handlers {
    /// The operations every node handles, and nothing else yet.
    pub(crate) fn new() -> Self {
        let mut handlers = Self {
            by_id: IndexMap::new(),
            by_type: IndexMap::new(),
        };
        ops::register(&mut handlers);
        handlers
    }

    /// Makes `M` deliverable to actors on this node. Registering it twice is
    /// the same as once.
    ///
    /// # Panics
    ///
    /// If another message is already registered under `M`'s
    /// [`MessageId`](crate::StableId::Id). See [`Handlers::claim`].
    pub(crate) fn insert<M: RemoteMessage>(&mut self) {
        self.claim::<M>(Arc::new(Typed::<M>(PhantomData)));
        self.by_type.insert(TypeId::of::<M>(), M::Id);
    }

    /// Makes the node handle the operation `M` itself, see [`ops`]. Not listed
    /// by type: an operation is for the actor's channel, not its mailbox.
    pub(super) fn insert_op<M: Operation>(&mut self) {
        self.claim::<M>(Arc::new(Builtin::<M>(PhantomData)));
    }

    /// Puts `handler` under `M`'s id, which `M` must be alone in claiming.
    ///
    /// A [`MessageId`] names a message to every node in the cluster, so two
    /// types under one id is not a collision this node can resolve: it decodes
    /// whatever arrives as one of them, silently delivering the wrong message
    /// or failing to decode. It is a mistake in the program — two types given
    /// the same uuid, or one copied from another — and not a state a running
    /// node can be left in, so it panics here, while the node is still being
    /// built.
    ///
    /// Registering the *same* type twice is harmless, since the second handler
    /// is the first one over again; it is worth saying so, because it usually
    /// means a message registered by hand as well as by
    /// [`auto_register`](crate::ClusterConfig::auto_register).
    ///
    /// # Panics
    ///
    /// If `M::Id` is already registered for a different type.
    fn claim<M: RemoteMessage>(&mut self, handler: Arc<dyn Handler>) {
        let type_id = TypeId::of::<M>();
        if let Some(taken) = self.by_id.get(&M::Id) {
            assert!(
                taken.type_id == type_id,
                "The message id {} is claimed by two types: `{}` and `{}`. \
                 A `StableId` names a message to the whole cluster, so it must \
                 be unique; give one of them a fresh uuid.",
                M::Id,
                taken.type_name,
                std::any::type_name::<M>(),
            );
            // Not named `message`: that is the field tracing puts the event's
            // own text in, and a second one would shadow it.
            tracing::warn!(
                id = %M::Id,
                message_type = std::any::type_name::<M>(),
                "A remote message was registered twice; the second registration changes nothing"
            );
        }
        self.by_id.insert(
            M::Id,
            Registered {
                handler,
                type_id,
                type_name: std::any::type_name::<M>(),
            },
        );
    }

    /// What delivers the message `id`, if this node accepts it at all.
    pub(super) fn get(&self, id: &MessageId) -> Option<&Arc<dyn Handler>> {
        self.by_id.get(id).map(|registered| &registered.handler)
    }

    /// The ids `address` accepts, among the messages registered here, sorted.
    pub(crate) fn accepted_ids(&self, address: &impl ActorRef) -> Vec<MessageId> {
        self.registered_ids(address.members())
    }

    /// Whether `address` accepts every message in `ids`, however many that is.
    ///
    /// Maps the actor's types once and looks each id up in the result, rather
    /// than rescanning the actor per id. An actor accepts few messages, so the
    /// list is short whatever `ids` asks about.
    pub(crate) fn accepts_ids(&self, address: &impl ActorRef, ids: &[MessageId]) -> bool {
        let accepted = self.registered_ids(address.members());
        ids.iter().all(|id| accepted.binary_search(id).is_ok())
    }

    /// The ids registered here for `members`, sorted. The one answer to "which
    /// of these types can this node name on the wire?", so that a question
    /// about an actor is answered the same way however it is asked.
    ///
    /// Walks the types given rather than every handler: an actor accepts few
    /// messages, a node may know many. The built-in operations are never
    /// listed — they are registered by id alone, since they are for the
    /// actor's channel and not its mailbox.
    fn registered_ids(&self, members: &[TypeId]) -> Vec<MessageId> {
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
    /// Carries the operation out on `address`. `peer` is the node that asked,
    /// which an operation outliving its message needs in order to be called off.
    fn run(
        self,
        address: Address,
        cluster: Cluster,
        peer: NodeName,
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
            let running = msg.run(
                address,
                wire.session().cluster().clone(),
                wire.peer().clone(),
            );
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StableId;
    use serde::{Deserialize, Serialize};
    use zestors_interface::Message;

    /// The uuid `Ask` and `Tell` both claim, which is the mistake under test.
    /// They opt out of `auto_register` so that they are never collected into a
    /// real node: they exist only to be registered by hand, here.
    const SHARED: &str = "0f7a1c2d-3e4b-4a5c-8d6e-7f8091a2b3c4";

    #[derive(Message, StableId, Serialize, Deserialize)]
    #[msg(reply = u32, id = "0f7a1c2d-3e4b-4a5c-8d6e-7f8091a2b3c4", no_auto_register)]
    #[zestors(interface_path = "zestors_interface", distr_path = "crate")]
    struct Ask(u32);

    #[derive(Message, StableId, Serialize, Deserialize)]
    #[msg(reply = u32, id = "0f7a1c2d-3e4b-4a5c-8d6e-7f8091a2b3c4", no_auto_register)]
    #[zestors(interface_path = "zestors_interface", distr_path = "crate")]
    struct Tell(u32);

    #[test]
    fn built_in_operations_have_distinct_ids() {
        // `new` panics if two of them share an id, which nothing else catches:
        // the second would silently replace the first, and the operation it
        // belongs to would answer with the wrong one's reply.
        let handlers = Handlers::new();
        assert!(handlers.by_id.len() >= 7);
    }

    #[test]
    fn registering_the_same_message_twice_is_the_same_as_once() {
        let mut handlers = Handlers::new();
        handlers.insert::<Ask>();
        let after_first = handlers.by_id.len();

        let warnings = capture_warnings(|| handlers.insert::<Ask>());

        assert_eq!(handlers.by_id.len(), after_first);
        assert!(handlers.get(&Ask::Id).is_some());
        assert_eq!(handlers.by_type.get(&TypeId::of::<Ask>()), Some(&Ask::Id));
        // Harmless, but said out loud: it usually means a message registered
        // by hand as well as by `auto_register`.
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("registered twice"), "{warnings:?}");
    }

    /// Runs `f` and returns the warnings it logs, so that a warning documented
    /// as part of the behaviour is tested like the rest of it.
    fn capture_warnings(f: impl FnOnce()) -> Vec<String> {
        use std::sync::{Arc, Mutex};
        use tracing::field::{Field, Visit};
        use tracing_subscriber::{Layer, layer::Context, prelude::*};

        #[derive(Clone, Default)]
        struct Warnings(Arc<Mutex<Vec<String>>>);

        impl<S: tracing::Subscriber> Layer<S> for Warnings {
            fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
                if *event.metadata().level() != tracing::Level::WARN {
                    return;
                }
                struct Message(String);
                impl Visit for Message {
                    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                        if field.name() == "message" {
                            self.0 = format!("{value:?}");
                        }
                    }
                }
                let mut message = Message(String::new());
                event.record(&mut message);
                self.0.lock().expect("Not poisoned").push(message.0);
            }
        }

        let warnings = Warnings::default();
        tracing::subscriber::with_default(
            tracing_subscriber::registry().with(warnings.clone()),
            f,
        );
        warnings.0.lock().expect("Not poisoned").clone()
    }

    #[test]
    #[should_panic(expected = "claimed by two types")]
    fn two_types_cannot_claim_the_same_message_id() {
        assert_eq!(Ask::Id.to_string(), SHARED);
        let mut handlers = Handlers::new();
        handlers.insert::<Ask>();
        handlers.insert::<Tell>();
    }

    #[test]
    #[should_panic(expected = "claimed by two types")]
    fn a_message_cannot_claim_a_built_in_operations_id() {
        #[derive(Message, StableId, Serialize, Deserialize)]
        #[msg(reply = bool, id = "b5c3f7a0-5f0e-4b0f-9f7e-2f6c1f0a0001", no_auto_register)]
        #[zestors(interface_path = "zestors_interface", distr_path = "crate")]
        struct Impostor;

        Handlers::new().insert::<Impostor>();
    }
}
