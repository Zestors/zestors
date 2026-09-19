//! The sending side: [`RemoteAddress`], and the calls waiting for their reply.

use super::{
    Remote, RemoteCallError, RemoteCastError, RemoteError, RemoteMessage, RemoteReplyError, SHARDS,
    wire::Frame,
};
use crate::{
    GlobalPid, NodeId,
    link::{Delivery, Protocol},
};
use bytes::Bytes;
use dashmap::DashMap;
use std::{
    fmt,
    future::Future,
    hash::{Hash, Hasher},
    marker::PhantomData,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot},
    time::timeout,
};
use type_sets::Contains;
use zestors_interface::Message;
use zestors_runtime::{Context, Dyn, Pid};

/// An actor on another node, that messages can be sent to: the remote analog of
/// [`Address`](zestors_runtime::Address).
///
/// Made with [`Remote::address`]. `C` is what the actor is expected to accept,
/// either its [`Interface`](zestors_interface::Interface) or a set of messages
/// like `Dyn<(Ping, Double)>`, and only those can be sent. Whether the actor
/// really does is up to the node it runs on to say: a message it doesn't accept
/// is answered with [`RemoteError::NotAccepted`].
///
/// Messages are sent with [`RemoteAccepts`]. Messages sent to one actor arrive
/// in the order they were sent. A message that is not answered is not sent
/// again; delivery is at most once.
pub struct RemoteAddress<C: Context = Dyn> {
    remote: Remote,
    target: GlobalPid,
    timeout: Option<Duration>,
    _ctx: PhantomData<fn() -> C>,
}

impl<C: Context> Clone for RemoteAddress<C> {
    fn clone(&self) -> Self {
        Self {
            remote: self.remote.clone(),
            target: self.target.clone(),
            timeout: self.timeout,
            _ctx: PhantomData,
        }
    }
}

impl<C: Context> fmt::Debug for RemoteAddress<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteAddress")
            .field("target", &self.target)
            .finish()
    }
}

/// Options for a single message sent with [`RemoteAccepts`], the remote
/// counterpart of [`CallOptions`](zestors_runtime::CallOptions).
///
/// ```
/// # use zestors_distr::RemoteCallOptions;
/// # use std::time::Duration;
/// let options = RemoteCallOptions::new().timeout(Duration::from_secs(2));
/// assert_eq!(options.timeout, Some(Duration::from_secs(2)));
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct RemoteCallOptions {
    /// How long to wait for the reply, instead of the address's or the node's
    /// default.
    pub timeout: Option<Duration>,
}

impl RemoteCallOptions {
    /// Creates a new [`RemoteCallOptions`] with nothing set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets [`RemoteCallOptions::timeout`].
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// Message-sending operations for a reference to a remote actor: the remote
/// counterpart of [`Accepts`](zestors_runtime::Accepts), and it works the same
/// way.
///
/// It is implemented for a [`RemoteAddress<C>`] for every [`RemoteMessage`] that
/// `C` accepts, so only messages the actor is expected to take can be sent.
/// Sending returns the message's [`RemoteMessage::RemoteReceipt`]: `()` for a
/// message that expects no reply, and a [`RemoteReply`] to wait for the reply
/// of one that does. [`call`](Self::call) sends and waits for it.
///
/// There are two ways to send:
///
/// - [`cast`](Self::cast) waits for room to send, and so only fails if the
///   message can't be sent at all.
/// - [`try_cast`](Self::try_cast) never waits: it also fails with
///   [`RemoteCastError::Full`] if many messages are queued for the node.
///
/// Returning means that the message is queued for sending, not that it arrived
/// or was accepted; for that, wait for the reply. Unlike a local message, one
/// can fail on the way: see [`RemoteReplyError`]. A message that got no answer
/// is not sent again; delivery is at most once.
pub trait RemoteAccepts<M: RemoteMessage>: Sync {
    /// Sends a message, waiting for room to send it if many messages are
    /// queued for the node.
    ///
    /// Equivalent to [`RemoteAccepts::cast_with`] with the default
    /// [`RemoteCallOptions`].
    fn cast(
        &self,
        msg: M,
    ) -> impl Future<Output = Result<M::RemoteReceipt, RemoteCastError<M>>> + Send {
        self.cast_with(msg, Default::default())
    }

    /// Same as [`RemoteAccepts::cast`], with explicit [`RemoteCallOptions`].
    fn cast_with(
        &self,
        msg: M,
        options: RemoteCallOptions,
    ) -> impl Future<Output = Result<M::RemoteReceipt, RemoteCastError<M>>> + Send;

    /// Sends a message immediately, without waiting for room.
    ///
    /// Equivalent to [`RemoteAccepts::try_cast_with`] with the default
    /// [`RemoteCallOptions`].
    fn try_cast(&self, msg: M) -> Result<M::RemoteReceipt, RemoteCastError<M>> {
        self.try_cast_with(msg, Default::default())
    }

    /// Same as [`RemoteAccepts::try_cast`], with explicit [`RemoteCallOptions`].
    ///
    /// Fails with [`RemoteCastError::Full`] if many messages are queued for the
    /// node.
    fn try_cast_with(
        &self,
        msg: M,
        options: RemoteCallOptions,
    ) -> Result<M::RemoteReceipt, RemoteCastError<M>>;

    /// Sends a message via [`RemoteAccepts::cast`] and waits for its reply.
    ///
    /// Equivalent to calling [`RemoteAccepts::cast`] and then
    /// [`RemoteReceipt::wait`] on the result, so it shares `cast`'s failures and
    /// adds those of getting the reply. The output is [`Message::Output`](zestors_interface::Message::Output), the
    /// reply. For a message that expects no reply, that is `()` as soon as the
    /// message is queued.
    fn call(&self, msg: M) -> impl Future<Output = Result<M::Output, RemoteCallError<M>>> + Send {
        self.call_with(msg, Default::default())
    }

    /// Same as [`RemoteAccepts::call`], with explicit [`RemoteCallOptions`].
    fn call_with(
        &self,
        msg: M,
        options: RemoteCallOptions,
    ) -> impl Future<Output = Result<M::Output, RemoteCallError<M>>> + Send {
        async move {
            let receipt = self.cast_with(msg, options).await?;
            receipt.wait().await.map_err(RemoteCallError::Reply)
        }
    }
}

impl<M, C> RemoteAccepts<M> for RemoteAddress<C>
where
    M: RemoteMessage,
    C: Context,
    C::Set: Contains<M>,
{
    async fn cast_with(
        &self,
        msg: M,
        options: RemoteCallOptions,
    ) -> Result<M::RemoteReceipt, RemoteCastError<M>> {
        let (prepared, waiting) = match self.prepare(&msg, options) {
            Ok(prepared) => prepared,
            Err(refusal) => return Err(refusal.with(msg)),
        };
        match prepared.pipe.send(prepared.frame).await {
            Ok(()) => Ok(M::remote_receipt(waiting)),
            Err(_) => Err(RemoteCastError::Unreachable(msg)),
        }
    }

    fn try_cast_with(
        &self,
        msg: M,
        options: RemoteCallOptions,
    ) -> Result<M::RemoteReceipt, RemoteCastError<M>> {
        let (prepared, waiting) = match self.prepare(&msg, options) {
            Ok(prepared) => prepared,
            Err(refusal) => return Err(refusal.with(msg)),
        };
        match prepared.pipe.try_send(prepared.frame) {
            Ok(()) => Ok(M::remote_receipt(waiting)),
            Err(mpsc::error::TrySendError::Full(_)) => Err(RemoteCastError::Full(msg)),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(RemoteCastError::Unreachable(msg)),
        }
    }
}

/// Why a message can't be sent, before it is known which message it is.
enum Refusal {
    NotRunning,
    NotAMember,
    Unreachable,
    TooLarge { size: usize, max: usize },
    Encode(super::EncodeError),
}

impl Refusal {
    fn with<M>(self, msg: M) -> RemoteCastError<M> {
        match self {
            Refusal::NotRunning => RemoteCastError::NotRunning(msg),
            Refusal::NotAMember => RemoteCastError::NotAMember(msg),
            Refusal::Unreachable => RemoteCastError::Unreachable(msg),
            Refusal::TooLarge { size, max } => RemoteCastError::TooLarge { msg, size, max },
            Refusal::Encode(error) => RemoteCastError::Encode { msg, error },
        }
    }
}

/// A message ready to be sent, and what to wait on for its reply, if it has one.
type Sending<M> = (Prepared, Option<RemoteReply<<M as Message>::Output>>);

/// A message, encoded and ready to go into the pipe to its node.
struct Prepared {
    pipe: mpsc::Sender<Bytes>,
    frame: Bytes,
}

impl<C: Context> RemoteAddress<C> {
    pub(super) fn new(remote: Remote, target: GlobalPid) -> Self {
        Self {
            remote,
            target,
            timeout: None,
            _ctx: PhantomData,
        }
    }

    /// The actor this address is for.
    pub fn target(&self) -> &GlobalPid {
        &self.target
    }

    /// How long [`RemoteAccepts::call`] and [`RemoteReceipt::wait`] wait for a
    /// reply, instead of the node's
    /// [`ClusterConfig::call_timeout`](crate::ClusterConfig::call_timeout).
    /// [`RemoteCallOptions::timeout`] overrides it for one call.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Encodes `msg` and finds the pipe to the node, as a call if the message
    /// expects a reply, else as a cast. For a call, what to wait on for the
    /// reply is set up, so that a reply can't beat it.
    fn prepare<M: RemoteMessage>(
        &self,
        msg: &M,
        options: RemoteCallOptions,
    ) -> Result<Sending<M>, Refusal> {
        let shared = &self.remote.shared;
        let running = shared.running().ok_or(Refusal::NotRunning)?;
        let member = shared
            .cluster
            .member(self.target.node())
            .ok_or(Refusal::NotAMember)?;
        if !shared.cluster.is_reachable(self.target.node()) {
            return Err(Refusal::Unreachable);
        }

        let payload = msg.encode().map_err(Refusal::Encode)?;
        let (target, id) = (self.target.pid().clone(), M::Id);
        let call_id = M::REPLIES.then(|| shared.next_call.fetch_add(1, Ordering::Relaxed));
        let frame = match call_id {
            Some(call_id) => Frame::Call {
                call_id,
                target,
                msg: id,
                payload,
            },
            None => Frame::Cast {
                target,
                msg: id,
                payload,
            },
        }
        .encode();

        let max = running.links.max_message_size();
        if frame.len() > max {
            return Err(Refusal::TooLarge {
                size: frame.len(),
                max,
            });
        }

        let waiting = call_id.map(|call_id| {
            let (tx, rx) = oneshot::channel();
            running.pending.insert(call_id, member.node.clone(), tx);
            RemoteReply {
                rx,
                _guard: PendingGuard {
                    pending: running.pending.clone(),
                    call_id,
                },
                timeout: options
                    .timeout
                    .or(self.timeout)
                    .unwrap_or(shared.call_timeout),
                decode: M::decode_output,
            }
        });

        let pipe = running.links.sender(
            &member,
            Protocol::ACTORS,
            Delivery::Ordered,
            shard_of(self.target.pid()),
        );
        Ok((Prepared { pipe, frame }, waiting))
    }
}

/// Which of a peer's pipes carries messages for `pid`. The same one every time,
/// so that messages to one actor stay in order, while different actors mostly
/// don't wait for each other.
fn shard_of(pid: &Pid) -> u8 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    pid.hash(&mut hasher);
    (hasher.finish() % SHARDS as u64) as u8
}

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
#[doc(hidden)]
pub struct RemoteReply<T> {
    rx: oneshot::Receiver<Result<Bytes, RemoteReplyError>>,
    _guard: PendingGuard,
    timeout: Duration,
    decode: fn(Bytes) -> Result<T, super::DecodeError>,
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
#[doc(hidden)]
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
    fn insert(
        &self,
        call_id: u64,
        node: NodeId,
        reply: oneshot::Sender<Result<Bytes, RemoteReplyError>>,
    ) {
        self.calls.insert(call_id, PendingCall { node, reply });
    }

    fn remove(&self, call_id: u64) {
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
