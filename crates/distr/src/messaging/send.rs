//! The sending side: [`RemoteAddress`], and how a message is put on its way.

use super::{
    Decode, NodeRef, RemoteCallOptions, RemoteCastError, RemoteMessage, RemoteOpError,
    RemoteReceipt as _, RemoteReply,
    context::{Exports, Wire},
    reply::RemoteMessageKind,
    wire::Frame,
};
use crate::{
    GlobalName,
    link::{Delivery, MAX_MESSAGE_SIZE, Protocol},
};
use bytes::Bytes;
use std::{
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    sync::atomic::Ordering,
    time::Duration,
};
use tokio::sync::mpsc;
use zestors_interface::Message;
use zestors_runtime::{Context, Dyn, Name};

/// An actor on another node, that messages can be sent to: the remote analog of
/// [`Address`](zestors_runtime::Address).
///
/// Made with [`Remote::address`]. `C` is what the actor is expected to accept,
/// either its [`Interface`](zestors_interface::Interface) or a set of messages
/// like `Dyn<(Ping, Double)>`, and only those can be sent. Whether the actor
/// really does is up to the node it runs on to say: a message it doesn't accept
/// is answered with [`RemoteError::NotAccepted`](super::RemoteError::NotAccepted).
///
/// Messages are sent with [`RemoteAccepts`](super::RemoteAccepts), and the actor
/// is operated on with [`RemoteActorOps`](super::RemoteActorOps). Messages sent to one actor arrive
/// in the order they were sent. A message that is not answered is not sent
/// again; delivery is at most once.
pub struct RemoteAddress<C: Context = Dyn> {
    node: NodeRef,
    target: GlobalName,
    timeout: Option<Duration>,
    _ctx: PhantomData<fn() -> C>,
}

impl<C: Context> Clone for RemoteAddress<C> {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
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

/// A message, encoded and ready to go into the lane to its node.
struct Prepared {
    lane: mpsc::Sender<Bytes>,
    frame: Bytes,
    exports: Exports,
}

impl<C: Context> RemoteAddress<C> {
    pub(super) fn new(node: NodeRef, target: GlobalName) -> Self {
        Self {
            node,
            target,
            timeout: None,
            _ctx: PhantomData,
        }
    }

    /// Sends `msg` to the actor on its node, waiting for room in the lane to it.
    pub(super) async fn cast_remote<M: RemoteMessage>(
        &self,
        msg: M,
        options: RemoteCallOptions,
    ) -> Result<M::RemoteReceipt, RemoteCastError<M>> {
        let (prepared, waiting) = match self.prepare(&msg, options) {
            Ok(prepared) => prepared,
            Err(refusal) => return Err(refusal.with(msg)),
        };
        match prepared.lane.send(prepared.frame).await {
            Ok(()) => {
                prepared.exports.commit();
                Ok(M::remote_receipt(waiting))
            }
            Err(_) => Err(RemoteCastError::Unreachable(msg)),
        }
    }

    /// Like [`RemoteAddress::cast_remote`], but fails if the lane is full.
    pub(super) fn try_cast_remote<M: RemoteMessage>(
        &self,
        msg: M,
        options: RemoteCallOptions,
    ) -> Result<M::RemoteReceipt, RemoteCastError<M>> {
        let (prepared, waiting) = match self.prepare(&msg, options) {
            Ok(prepared) => prepared,
            Err(refusal) => return Err(refusal.with(msg)),
        };
        match prepared.lane.try_send(prepared.frame) {
            Ok(()) => {
                prepared.exports.commit();
                Ok(M::remote_receipt(waiting))
            }
            Err(mpsc::error::TrySendError::Full(_)) => Err(RemoteCastError::Full(msg)),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(RemoteCastError::Unreachable(msg)),
        }
    }

    /// The actor this address is for.
    pub fn target(&self) -> &GlobalName {
        &self.target
    }

    /// How long [`RemoteAccepts::call`](super::RemoteAccepts::call) and [`RemoteReceipt::wait`](super::RemoteReceipt::wait) wait for a
    /// reply, instead of the node's
    /// [`ClusterConfig::call_timeout`](crate::ClusterConfig::call_timeout).
    /// [`RemoteCallOptions::timeout`] overrides it for one call.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Calls one of the [operations](super::ops) on the actor, whatever the
    /// actor accepts.
    pub(super) async fn call_op<M: RemoteMessage>(
        &self,
        msg: M,
    ) -> Result<M::Output, RemoteOpError> {
        let (prepared, waiting) = match self.prepare(&msg, Default::default()) {
            Ok(prepared) => prepared,
            Err(refusal) => return Err(refusal.with(()).into()),
        };
        if prepared.lane.send(prepared.frame).await.is_err() {
            return Err(RemoteCastError::Unreachable(()).into());
        }
        prepared.exports.commit();
        Ok(M::remote_receipt(waiting).wait().await?)
    }

    /// Encodes `msg` and finds the lane to the node, as a call if the message
    /// expects a reply, else as a cast. For a call, what to wait on for the
    /// reply is set up, so that a reply can't beat it.
    fn prepare<M: RemoteMessage>(
        &self,
        msg: &M,
        options: RemoteCallOptions,
    ) -> Result<Sending<M>, Refusal> {
        let shared = &self.node.shared;
        let running = shared.running().ok_or(Refusal::NotRunning)?;
        let member = shared
            .cluster
            .member(self.target.node())
            .ok_or(Refusal::NotAMember)?;
        if !shared.cluster.is_reachable(self.target.node()) {
            return Err(Refusal::Unreachable);
        }

        let wire = Wire::new(
            self.node.shared.clone(),
            running.clone(),
            member.node.clone(),
        );
        let payload = wire.scope(|| msg.encode());
        // Forgets the requests in the message again if it isn't sent.
        let exports = wire.exports();
        let payload = payload.map_err(Refusal::Encode)?;
        let (target, id) = (self.target.name().clone(), M::Id);
        let call_id = <M::Kind as RemoteMessageKind<M::Output>>::REPLIES
            .then(|| shared.next_call.fetch_add(1, Ordering::Relaxed));
        let frame = match call_id {
            Some(call_id) => Frame::Call {
                call_id,
                target,
                msg: id,
                requests: exports.ids(),
                payload,
            },
            None => Frame::Cast {
                target,
                msg: id,
                requests: exports.ids(),
                payload,
            },
        }
        .encode();

        if frame.len() > MAX_MESSAGE_SIZE {
            return Err(Refusal::TooLarge {
                size: frame.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }

        let waiting = call_id.map(|call_id| {
            let timeout = options
                .timeout
                .or(self.timeout)
                .unwrap_or(shared.call_timeout);
            running.pending.expect(
                call_id,
                member.node.clone(),
                timeout,
                <M::Output as Decode>::decode,
            )
        });

        let lane = running.links.sender(
            &member.node,
            &member.addr,
            Protocol::ACTORS,
            Delivery::Ordered,
            shard_of(self.target.name(), shared.shards),
        );
        Ok((
            Prepared {
                lane,
                frame,
                exports,
            },
            waiting,
        ))
    }
}

/// Which of a peer's lanes carries messages for `name`. The same one every time,
/// so that messages to one actor stay in order, while different actors mostly
/// don't wait for each other.
fn shard_of(name: &Name, shards: u8) -> u8 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    (hasher.finish() % shards as u64) as u8
}
