//! Delivers a message to a local actor, knowing its type.

use super::Cluster;
use super::{Encode, RemoteError, RemoteMessage, context::Wire};
use bytes::Bytes;
use std::{future::Future, marker::PhantomData, pin::Pin};
use zestors_interface::Receipt;
use zestors_runtime::{Address, errors::CastDynError, prelude::*};

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
            let running = msg.run(address, wire.cluster().clone());
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
