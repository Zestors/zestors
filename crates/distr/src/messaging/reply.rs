//! Waiting for a reply: what a sent message gives back.

use super::{DecodeError, RemoteError, RemoteReplyError, pending::PendingGuard};
use bytes::Bytes;
use std::{future::Future, time::Duration};
use tokio::{sync::oneshot, time::timeout};

/// What a sent message gives back to wait on, the remote counterpart of
/// [`Receipt`](zestors_interface::Receipt): `()` for a message that expects no
/// reply, and a [`RemoteReply`] for one that does.
pub trait RemoteReceipt: Send + Sized {
    /// What waiting results in: the message's [`Message::Output`](zestors_interface::Message::Output).
    type Output: Send + 'static;

    /// Waits for the message's outcome.
    fn wait(self) -> impl Future<Output = Result<Self::Output, RemoteReplyError>> + Send;
}

impl RemoteReceipt for () {
    type Output = ();

    async fn wait(self) -> Result<(), RemoteReplyError> {
        Ok(())
    }
}

impl<T: Send + 'static> RemoteReceipt for RemoteReply<T> {
    type Output = T;

    async fn wait(self) -> Result<T, RemoteReplyError> {
        self.wait().await
    }
}

/// A reply that is being waited for.
pub struct RemoteReply<T>(pub(super) Source<T>);

pub(super) enum Source<T> {
    /// Comes from another node.
    Remote {
        rx: oneshot::Receiver<Result<Bytes, RemoteReplyError>>,
        _guard: PendingGuard,
        timeout: Duration,
        decode: fn(Bytes) -> Result<T, DecodeError>,
    },
    /// Comes from an actor on this node.
    Local {
        reply: zestors_interface::Reply<T>,
        timeout: Option<Duration>,
    },
}

impl<T> RemoteReply<T> {
    /// The reply of a message to an actor on this node.
    pub(super) fn local(reply: zestors_interface::Reply<T>, timeout: Option<Duration>) -> Self {
        Self(Source::Local { reply, timeout })
    }

    async fn wait(self) -> Result<T, RemoteReplyError> {
        match self.0 {
            Source::Remote {
                rx,
                timeout: limit,
                decode,
                ..
            } => match timeout(limit, rx).await {
                Ok(Ok(Ok(bytes))) => decode(bytes).map_err(RemoteReplyError::Decode),
                Ok(Ok(Err(error))) => Err(error),
                // Whoever would have answered is gone.
                Ok(Err(_)) => Err(RemoteReplyError::Disconnected),
                Err(_) => Err(RemoteReplyError::Timeout),
            },
            Source::Local {
                reply,
                timeout: limit,
            } => {
                // The actor dropped the request without replying.
                let reply = async { reply.await.map_err(|_| RemoteError::NoReply.into()) };
                match limit {
                    Some(limit) => timeout(limit, reply)
                        .await
                        .unwrap_or(Err(RemoteReplyError::Timeout)),
                    None => reply.await,
                }
            }
        }
    }
}
