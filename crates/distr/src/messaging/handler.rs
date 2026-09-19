//! Delivers a message to a local actor, knowing its type.

use super::{Encode, RemoteError, RemoteMessage, context::Wire};
use bytes::Bytes;
use std::{future::Future, marker::PhantomData, pin::Pin};
use zestors_interface::Receipt;
use zestors_runtime::{Address, errors::CastDynError, prelude::*};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

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
}

pub(super) struct Typed<M>(pub(super) PhantomData<fn() -> M>);

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
