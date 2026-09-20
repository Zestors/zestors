//! Waiting for a reply: what a sent message gives back.

use super::{ClusterReplyError, DecodeError, RemoteError, pending::PendingGuard};
use bytes::Bytes;
use std::{future::Future, time::Duration};
use tokio::{sync::oneshot, time::timeout};

/// What a sent message gives back to wait on, the cluster counterpart of
/// [`Receipt`](zestors_interface::Receipt): `()` for a message that expects no
/// reply, and a [`ClusterReply`] for one that does.
pub trait ClusterReceipt: Send + Sized {
    /// What waiting results in: the message's [`Message::Output`](zestors_interface::Message::Output).
    type Output: Send + 'static;

    /// Waits for the message's outcome.
    fn wait(self) -> impl Future<Output = Result<Self::Output, ClusterReplyError>> + Send;
}

impl ClusterReceipt for () {
    type Output = ();

    async fn wait(self) -> Result<(), ClusterReplyError> {
        Ok(())
    }
}

impl<T: Send + 'static> ClusterReceipt for ClusterReply<T> {
    type Output = T;

    async fn wait(self) -> Result<T, ClusterReplyError> {
        self.wait().await
    }
}

/// A reply that is being waited for.
pub struct ClusterReply<T>(pub(super) Source<T>);

pub(super) enum Source<T> {
    /// Comes from another node.
    Remote {
        rx: oneshot::Receiver<Result<Bytes, ClusterReplyError>>,
        _guard: PendingGuard,
        timeout: Option<Duration>,
        decode: fn(Bytes) -> Result<T, DecodeError>,
    },
    /// Comes from an actor on this node.
    Local {
        reply: zestors_interface::Reply<T>,
        timeout: Option<Duration>,
    },
}

impl<T> ClusterReply<T> {
    /// The reply of a message to an actor on this node.
    pub(super) fn local(reply: zestors_interface::Reply<T>, timeout: Option<Duration>) -> Self {
        Self(Source::Local { reply, timeout })
    }

    async fn wait(self) -> Result<T, ClusterReplyError> {
        match self.0 {
            Source::Remote {
                rx,
                timeout: limit,
                decode,
                ..
            } => {
                let answered = async {
                    match rx.await {
                        Ok(Ok(bytes)) => decode(bytes).map_err(ClusterReplyError::Decode),
                        Ok(Err(error)) => Err(error),
                        // Whoever would have answered is gone.
                        Err(_) => Err(ClusterReplyError::Disconnected),
                    }
                };
                match limit {
                    Some(limit) => timeout(limit, answered)
                        .await
                        .unwrap_or(Err(ClusterReplyError::Timeout)),
                    None => answered.await,
                }
            }
            Source::Local {
                reply,
                timeout: limit,
            } => {
                // The actor dropped the request without replying.
                let reply = async { reply.await.map_err(|_| RemoteError::NoReply.into()) };
                match limit {
                    Some(limit) => timeout(limit, reply)
                        .await
                        .unwrap_or(Err(ClusterReplyError::Timeout)),
                    None => reply.await,
                }
            }
        }
    }
}
