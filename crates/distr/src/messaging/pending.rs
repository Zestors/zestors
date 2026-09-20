//! The calls that have been sent to other nodes and are waiting for a reply.

use super::{ClusterReply, ClusterReplyError, DecodeError, RemoteError, reply::Source};
use crate::NodeName;
use bytes::Bytes;
use dashmap::DashMap;
use std::{sync::Arc, time::Duration};
use tokio::sync::oneshot;

/// Forgets a call once it is no longer waited for, however that happens.
pub(super) struct PendingGuard {
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
    node: NodeName,
    reply: oneshot::Sender<Result<Bytes, ClusterReplyError>>,
}

impl Pending {
    pub(super) fn insert(
        &self,
        call_id: u64,
        node: NodeName,
        reply: oneshot::Sender<Result<Bytes, ClusterReplyError>>,
    ) {
        self.calls.insert(call_id, PendingCall { node, reply });
    }

    /// Starts waiting for `node` to answer the call `call_id`. The call is
    /// forgotten when the [`ClusterReply`] is dropped, however that happens.
    ///
    /// `timeout` of `None` waits for as long as it takes, which a monitor does:
    /// losing the node still ends it, through [`Pending::fail_node`].
    pub(super) fn expect<T>(
        self: &Arc<Self>,
        call_id: u64,
        node: NodeName,
        timeout: Option<Duration>,
        decode: fn(Bytes) -> Result<T, DecodeError>,
    ) -> ClusterReply<T> {
        let (tx, rx) = oneshot::channel();
        self.insert(call_id, node, tx);
        ClusterReply(Source::Remote {
            rx,
            _guard: PendingGuard {
                pending: self.clone(),
                call_id,
            },
            timeout,
            decode,
        })
    }

    pub(super) fn remove(&self, call_id: u64) {
        self.calls.remove(&call_id);
    }

    /// `node` answered the call `call_id`.
    pub(super) fn complete(
        &self,
        node: &NodeName,
        call_id: u64,
        result: Result<Bytes, RemoteError>,
    ) {
        match self.calls.remove_if(&call_id, |_, call| call.node == *node) {
            Some((_, call)) => {
                let _ = call.reply.send(result.map_err(ClusterReplyError::Remote));
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
    pub(super) fn fail_node(&self, node: &NodeName) {
        let failed: Vec<u64> = self
            .calls
            .iter()
            .filter(|call| call.node == *node)
            .map(|call| *call.key())
            .collect();
        for id in failed {
            if let Some((_, call)) = self.calls.remove_if(&id, |_, call| call.node == *node) {
                let _ = call.reply.send(Err(ClusterReplyError::Disconnected));
            }
        }
    }

    /// Nothing more will be answered.
    pub(super) fn fail_all(&self) {
        let all: Vec<u64> = self.calls.iter().map(|call| *call.key()).collect();
        for id in all {
            if let Some((_, call)) = self.calls.remove(&id) {
                let _ = call.reply.send(Err(ClusterReplyError::Disconnected));
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
    ) -> oneshot::Receiver<Result<Bytes, ClusterReplyError>> {
        let (tx, rx) = oneshot::channel();
        pending.insert(id, NodeName::new(node), tx);
        rx
    }

    #[tokio::test]
    async fn a_call_is_answered_by_the_node_it_went_to() {
        let pending = Pending::default();
        let rx = call(&pending, 1, "node-a");
        pending.complete(&NodeName::new("node-a"), 1, Ok(Bytes::from_static(b"yes")));
        assert_eq!(rx.await.unwrap().unwrap(), "yes");
        assert_eq!(pending.len(), 0);
    }

    #[tokio::test]
    async fn a_reply_from_another_node_is_ignored() {
        let pending = Pending::default();
        let mut rx = call(&pending, 1, "node-a");
        pending.complete(&NodeName::new("node-b"), 1, Ok(Bytes::new()));
        assert!(rx.try_recv().is_err(), "Not answered");
        assert_eq!(pending.len(), 1, "Still waiting for node-a");
    }

    #[tokio::test]
    async fn losing_a_node_fails_only_its_calls() {
        let pending = Pending::default();
        let (lost, kept) = (call(&pending, 1, "node-a"), call(&pending, 2, "node-b"));
        pending.fail_node(&NodeName::new("node-a"));
        assert!(matches!(
            lost.await.unwrap(),
            Err(ClusterReplyError::Disconnected)
        ));
        assert_eq!(pending.len(), 1);
        pending.fail_all();
        assert!(matches!(
            kept.await.unwrap(),
            Err(ClusterReplyError::Disconnected)
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
