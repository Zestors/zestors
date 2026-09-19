//! Waiting for replies: what a sent message gives back, and the calls that
//! have been sent and are not yet answered.

use super::{DecodeError, RemoteError, RemoteReplyError};
use crate::NodeId;
use bytes::Bytes;
use dashmap::DashMap;
use std::{future::Future, sync::Arc, time::Duration};
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
pub struct RemoteReply<T> {
    rx: oneshot::Receiver<Result<Bytes, RemoteReplyError>>,
    _guard: PendingGuard,
    timeout: Duration,
    decode: fn(Bytes) -> Result<T, DecodeError>,
}

impl<T> RemoteReply<T> {
    async fn wait(self) -> Result<T, RemoteReplyError> {
        match timeout(self.timeout, self.rx).await {
            Ok(Ok(Ok(bytes))) => (self.decode)(bytes).map_err(RemoteReplyError::Decode),
            Ok(Ok(Err(error))) => Err(error),
            // Whoever would have answered is gone.
            Ok(Err(_)) => Err(RemoteReplyError::Disconnected),
            Err(_) => Err(RemoteReplyError::Timeout),
        }
    }
}

/// How the [`Receipt`](zestors_interface::Receipt) of a message, `()` or
/// [`Reply<T>`](zestors_interface::Reply), is sent and received remotely. The
/// two are all there are.
pub trait RemoteKind: zestors_interface::Receipt {
    type Remote: RemoteReceipt<Output = Self::Output>;

    /// Whether the message gets a reply.
    const REPLIES: bool;

    /// The remote receipt, given what to wait on if there is a reply.
    fn remote(waiting: Option<RemoteReply<Self::Output>>) -> Self::Remote;
}

impl RemoteKind for () {
    type Remote = ();
    const REPLIES: bool = false;

    fn remote(_: Option<RemoteReply<()>>) {}
}

impl<T: Send + 'static> RemoteKind for zestors_interface::Reply<T> {
    type Remote = RemoteReply<T>;
    const REPLIES: bool = true;

    fn remote(waiting: Option<RemoteReply<T>>) -> RemoteReply<T> {
        waiting.expect("A message with a reply is sent as a call")
    }
}

/// Forgets a call once it is no longer waited for, however that happens.
struct PendingGuard {
    pending: Arc<Pending>,
    call_id: u64,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.pending.remove(self.call_id);
    }
}

/// The calls that have been sent and are waiting for a reply.
#[derive(Default)]
pub(super) struct Pending {
    calls: DashMap<u64, PendingCall>,
}

struct PendingCall {
    /// The node the call went to, and the only one that may answer it.
    node: NodeId,
    reply: oneshot::Sender<Result<Bytes, RemoteReplyError>>,
}

impl Pending {
    pub(super) fn insert(
        &self,
        call_id: u64,
        node: NodeId,
        reply: oneshot::Sender<Result<Bytes, RemoteReplyError>>,
    ) {
        self.calls.insert(call_id, PendingCall { node, reply });
    }

    /// Starts waiting for `node` to answer the call `call_id`. The call is
    /// forgotten when the [`RemoteReply`] is dropped, however that happens.
    pub(super) fn expect<T>(
        self: &Arc<Self>,
        call_id: u64,
        node: NodeId,
        timeout: Duration,
        decode: fn(Bytes) -> Result<T, DecodeError>,
    ) -> RemoteReply<T> {
        let (tx, rx) = oneshot::channel();
        self.insert(call_id, node, tx);
        RemoteReply {
            rx,
            _guard: PendingGuard {
                pending: self.clone(),
                call_id,
            },
            timeout,
            decode,
        }
    }

    pub(super) fn remove(&self, call_id: u64) {
        self.calls.remove(&call_id);
    }

    /// `node` answered the call `call_id`.
    pub(super) fn complete(&self, node: &NodeId, call_id: u64, result: Result<Bytes, RemoteError>) {
        match self.calls.remove_if(&call_id, |_, call| call.node == *node) {
            Some((_, call)) => {
                let _ = call.reply.send(result.map_err(RemoteReplyError::Remote));
            }
            None if self.calls.contains_key(&call_id) => tracing::warn!(
                %node,
                call_id,
                "Dropping a reply from a node the call didn't go to"
            ),
            None => tracing::debug!(
                %node,
                call_id,
                "Dropping a reply to a call that is no longer waited for"
            ),
        }
    }

    /// The node is gone: the calls that went to it will not be answered.
    pub(super) fn fail_node(&self, node: &NodeId) {
        let failed: Vec<u64> = self
            .calls
            .iter()
            .filter(|call| call.node == *node)
            .map(|call| *call.key())
            .collect();
        for id in failed {
            if let Some((_, call)) = self.calls.remove_if(&id, |_, call| call.node == *node) {
                let _ = call.reply.send(Err(RemoteReplyError::Disconnected));
            }
        }
    }

    /// Nothing more will be answered.
    pub(super) fn fail_all(&self) {
        let all: Vec<u64> = self.calls.iter().map(|call| *call.key()).collect();
        for id in all {
            if let Some((_, call)) = self.calls.remove(&id) {
                let _ = call.reply.send(Err(RemoteReplyError::Disconnected));
            }
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.calls.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(
        pending: &Pending,
        id: u64,
        node: &str,
    ) -> oneshot::Receiver<Result<Bytes, RemoteReplyError>> {
        let (tx, rx) = oneshot::channel();
        pending.insert(id, NodeId::new(node), tx);
        rx
    }

    #[tokio::test]
    async fn a_call_is_answered_by_the_node_it_went_to() {
        let pending = Pending::default();
        let rx = call(&pending, 1, "node-a");
        pending.complete(&NodeId::new("node-a"), 1, Ok(Bytes::from_static(b"yes")));
        assert_eq!(rx.await.unwrap().unwrap(), "yes");
        assert_eq!(pending.len(), 0);
    }

    #[tokio::test]
    async fn a_reply_from_another_node_is_ignored() {
        let pending = Pending::default();
        let mut rx = call(&pending, 1, "node-a");
        pending.complete(&NodeId::new("node-b"), 1, Ok(Bytes::new()));
        assert!(rx.try_recv().is_err(), "Not answered");
        assert_eq!(pending.len(), 1, "Still waiting for node-a");
    }

    #[tokio::test]
    async fn losing_a_node_fails_only_its_calls() {
        let pending = Pending::default();
        let (lost, kept) = (call(&pending, 1, "node-a"), call(&pending, 2, "node-b"));
        pending.fail_node(&NodeId::new("node-a"));
        assert!(matches!(
            lost.await.unwrap(),
            Err(RemoteReplyError::Disconnected)
        ));
        assert_eq!(pending.len(), 1);
        pending.fail_all();
        assert!(matches!(
            kept.await.unwrap(),
            Err(RemoteReplyError::Disconnected)
        ));
        assert_eq!(pending.len(), 0);
    }

    #[tokio::test]
    async fn a_call_no_longer_waited_for_is_forgotten() {
        let pending = Arc::new(Pending::default());
        let _rx = call(&pending, 1, "node-a");
        drop(PendingGuard {
            pending: pending.clone(),
            call_id: 1,
        });
        assert_eq!(pending.len(), 0);
    }
}
