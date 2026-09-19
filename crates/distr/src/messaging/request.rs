//! [`RemoteRequest`]: a reply channel that can be part of a message sent to
//! another node.

use super::{Decode, Encode, wire};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser};
use std::{fmt, sync::Mutex};
use zestors_interface::{Reply, Request, ResolveError};

/// A [`Request`] that can be a field of a message sent to an actor on another
/// node.
///
/// A `Request` is a channel within one process. When a message holding a
/// `RemoteRequest` is sent to another node, the request stays where it was
/// made and the actor gets one that stands in for it: what it replies with is
/// sent back, and answers the original. It works the same locally, where it is
/// just a [`Request`].
///
/// ```
/// use serde::{Deserialize, Serialize};
/// use zestors_distr::RemoteRequest;
/// use zestors_interface::Message;
///
/// #[derive(Message, zestors_distr::StableId, Serialize, Deserialize, Debug)]
/// #[msg(id = "5a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d")]
/// struct Fetch {
///     key: String,
///     reply: RemoteRequest<String>,
/// }
///
/// // Keep the `Reply`, send the `RemoteRequest` along in the message.
/// let (request, reply) = RemoteRequest::new();
/// let message = Fetch { key: "a".into(), reply: request };
/// # drop((message, reply));
/// ```
///
/// The reply, `T`, is sent as any message is: it must be [`Encode`] and
/// [`Decode`], which every serde type is.
///
/// Things to know:
/// - Sending the message takes the request out of it. If the message is not
///   sent after all, the request is dropped, and its [`Reply`] fails.
/// - The [`Reply`] fails too if the actor drops the request, or the node it was
///   sent to is lost. There is no timeout: like a local request, it waits
///   until it is answered.
/// - Only messages can hold one: it can't be serialised outside of sending or
///   receiving a message, and it can't be part of a reply.
pub struct RemoteRequest<T>(Mutex<Option<Request<T>>>);

impl<T> RemoteRequest<T> {
    /// Creates a request and the [`Reply`] that it is answered on, like
    /// [`Request::new`].
    pub fn new() -> (Self, Reply<T>) {
        let (request, reply) = Request::new();
        (request.into(), reply)
    }

    /// Sends the reply, resolving the [`Reply`] on the other end.
    pub fn reply(self, value: T) -> Result<(), ResolveError<T>> {
        match self.into_request() {
            Some(request) => request.reply(value),
            // Already sent to another node: nothing to answer.
            None => Err(ResolveError(value)),
        }
    }

    /// Drops the request without a reply, and without logging that it was dropped.
    pub fn no_reply(self) {
        if let Some(request) = self.into_request() {
            request.no_reply();
        }
    }

    /// The [`Request`], unless it was sent to another node.
    pub fn into_request(self) -> Option<Request<T>> {
        self.0.into_inner().expect("Not poisoned")
    }

    /// Whether the [`Reply`] has been dropped, or the request was sent away.
    pub fn is_closed(&self) -> bool {
        self.0
            .lock()
            .expect("Not poisoned")
            .as_ref()
            .is_none_or(|request| request.is_closed())
    }
}

impl<T> From<Request<T>> for RemoteRequest<T> {
    fn from(request: Request<T>) -> Self {
        Self(Mutex::new(Some(request)))
    }
}

impl<T> fmt::Debug for RemoteRequest<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteRequest").finish()
    }
}

/// Goes on the wire as the id of the request, which the receiving node
/// answers to. Only while a message is sent.
impl<T: Decode + Send + 'static> Serialize for RemoteRequest<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = wire::current().ok_or_else(|| {
            ser::Error::custom("A RemoteRequest can only be serialized as part of a sent message")
        })?;
        let request = self
            .0
            .lock()
            .expect("Not poisoned")
            .take()
            .ok_or_else(|| ser::Error::custom("The RemoteRequest was already sent"))?;
        serializer.serialize_u64(wire.export(request))
    }
}

/// Reads the id of the request the sending node made. Only while a message is
/// received.
impl<'de, T: Encode + Send + 'static> Deserialize<'de> for RemoteRequest<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = u64::deserialize(deserializer)?;
        let wire = wire::current().ok_or_else(|| {
            de::Error::custom("A RemoteRequest can only be deserialized from a received message")
        })?;
        Ok(wire.import(id).into())
    }
}
