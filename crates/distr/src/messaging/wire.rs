//! What serialising a message needs to know about the node it goes to or comes
//! from, for the parts of a message that can't be plain data: a
//! [`RemoteRequest`](super::RemoteRequest) is a reply channel, which is
//! stood in for by a request id on the wire and answered over the connection.
//!
//! serde gives its impls no way to pass this along, so it is made available for
//! the duration of the encoding or decoding of one message, in a thread-local.
//! Both are synchronous, so nothing else can observe it in between.

use super::{Decode, Encode, RemoteError, pending::Pending, receive::Session};
use crate::NodeName;
use std::{
    cell::RefCell,
    sync::{Arc, Mutex, atomic::Ordering},
};
use tokio::sync::oneshot;
use zestors_interface::Request;

thread_local! {
    static CURRENT: RefCell<Option<Wire>> = const { RefCell::new(None) };
}

/// The node a message is being encoded for, or was decoded from.
#[derive(Clone)]
pub(super) struct Wire {
    session: Session,
    peer: NodeName,
    /// The requests handed to the node while encoding: see [`Wire::export`].
    exported: Arc<Mutex<Vec<u64>>>,
}

/// Puts back what was current before a [`Wire::scope`], however it ends.
struct Restore(Option<Wire>);

impl Drop for Restore {
    fn drop(&mut self) {
        CURRENT.with(|current| *current.borrow_mut() = self.0.take());
    }
}

/// The wire of the message being encoded or decoded right now, if any.
pub(super) fn current() -> Option<Wire> {
    CURRENT.with(|current| current.borrow().clone())
}

impl Wire {
    pub(super) fn new(session: Session, peer: NodeName) -> Self {
        Self {
            session,
            peer,
            exported: Arc::default(),
        }
    }

    pub(super) fn session(&self) -> &Session {
        &self.session
    }

    /// Runs `f`, which encodes or decodes one message, with this as its [`current`] wire.
    pub(super) fn scope<R>(&self, f: impl FnOnce() -> R) -> R {
        let previous = CURRENT.with(|current| current.borrow_mut().replace(self.clone()));
        let _restore = Restore(previous);
        f()
    }

    /// What was exported so far, to be forgotten again unless
    /// [`Exports::commit`]ted: the message may not make it out after all.
    pub(super) fn exports(&self) -> Exports {
        Exports {
            pending: self.session.pending().clone(),
            ids: std::mem::take(&mut *self.exported.lock().expect("Not poisoned")),
        }
    }

    /// Hands `request` to the node: what it replies with is sent back and
    /// answers `request`. Returns the id the node knows it by.
    ///
    /// It is answered by whoever is on the other end of the connection, so a
    /// reply is accepted only from the node it went to. If that node is lost,
    /// or answers with an error, `request` is dropped without a reply.
    ///
    /// It gets the node's
    /// [`call_timeout`](crate::ClusterConfig::call_timeout), as a call does:
    /// an actor that keeps a request rather than answering it would otherwise
    /// leave the sender waiting for good, and the entry waiting for it behind.
    pub(super) fn export<T: Decode + Send + 'static>(&self, request: Request<T>) -> u64 {
        let messaging = self.session.cluster().messaging();
        let id = messaging.next_call.fetch_add(1, Ordering::Relaxed);
        let deadline = messaging.call_timeout;
        let (answer, answered) = oneshot::channel();
        self.session.pending().insert(id, self.peer.clone(), answer);
        self.exported.lock().expect("Not poisoned").push(id);

        let pending = self.session.pending().clone();
        tokio::spawn(async move {
            match tokio::time::timeout(deadline, answered).await {
                Ok(Ok(Ok(bytes))) => match T::decode(bytes) {
                    Ok(value) => {
                        let _ = request.reply(value);
                    }
                    Err(error) => {
                        tracing::warn!("Failed to decode the reply to a request: {error}");
                        request.no_reply();
                    }
                },
                Ok(_) => request.no_reply(),
                Err(_) => {
                    tracing::debug!(id, "A request was not answered in time");
                    // Nothing is waiting for it any more, so stop expecting it.
                    pending.remove(id);
                    request.no_reply();
                }
            }
        });
        id
    }

    /// The request that the node knows by `id`, as a request on this node:
    /// what it is replied to is sent to the node. Dropping it without a reply
    /// tells the node so.
    pub(super) fn import<T: Encode + Send + 'static>(&self, id: u64) -> Request<T> {
        let (request, reply) = Request::<T>::new();
        let (session, peer) = (self.session.clone(), self.peer.clone());
        tokio::spawn(async move {
            let result = match reply.await {
                Ok(value) => value
                    .encode()
                    .map_err(|error| RemoteError::Encode(error.to_string())),
                Err(_) => Err(RemoteError::NoReply),
            };
            session.reply(&peer, id, result).await;
        });
        request
    }
}

/// The requests handed to a node with a message. Forgets them when dropped,
/// unless the message was sent.
pub(super) struct Exports {
    pending: Arc<Pending>,
    ids: Vec<u64>,
}

impl Exports {
    /// The ids of the requests, to put in the frame the message goes in.
    pub(super) fn ids(&self) -> Vec<u64> {
        self.ids.clone()
    }

    /// The message is on its way: the node will answer.
    pub(super) fn commit(mut self) {
        self.ids.clear();
    }
}

impl Drop for Exports {
    fn drop(&mut self) {
        for id in &self.ids {
            self.pending.remove(*id);
        }
    }
}
