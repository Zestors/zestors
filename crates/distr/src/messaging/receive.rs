//! The receiving side: messages from other nodes are decoded and delivered to
//! local actors, and answered.

use super::{
    RemoteError, SHARDS, Shared, Started, context::Wire, handler::ReplyFuture, reply::Pending,
    wire::Frame,
};
use crate::{
    ClusterEvent, Id, NodeId,
    link::{Delivery, Incoming, Links, MAX_MESSAGE_SIZE, PeerEvent, Protocol},
};
use bytes::Bytes;
use std::{
    collections::HashMap,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::{
    sync::{broadcast, mpsc},
    time::timeout,
};
use zestors_runtime::Name;

/// How much may wait for one actor before further messages are refused: bursts
/// are taken in, sustained overload is not. Bounded by size and not only by
/// count, since a message can be megabytes.
const ACTOR_QUEUE_MESSAGES: usize = 100_000;
const ACTOR_QUEUE_BYTES: usize = 16 * 1024 * 1024;
/// How long a task for an actor lingers without messages for it.
const ACTOR_IDLE: Duration = Duration::from_secs(60);

/// What waits for one actor, so that it can be bounded.
#[derive(Default)]
struct Queued {
    messages: AtomicUsize,
    bytes: AtomicUsize,
}

impl Queued {
    /// Makes room for a message of `len` bytes, unless too much is waiting
    /// already. One message is always let in, however large. Only the router
    /// admits, so checking and adding needn't be one step.
    fn admit(&self, len: usize) -> bool {
        let messages = self.messages.load(Ordering::Relaxed);
        let bytes = self.bytes.load(Ordering::Relaxed);
        if messages > 0 && (messages >= ACTOR_QUEUE_MESSAGES || bytes + len > ACTOR_QUEUE_BYTES) {
            return false;
        }
        self.messages.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(len, Ordering::Relaxed);
        true
    }

    /// A message of `len` bytes has been taken.
    fn release(&self, len: usize) {
        self.messages.fetch_sub(1, Ordering::Relaxed);
        self.bytes.fetch_sub(len, Ordering::Relaxed);
    }
}

/// The way to the task of one actor.
struct Route {
    queue: mpsc::UnboundedSender<Work>,
    queued: Arc<Queued>,
}

/// A message for a local actor, and where its reply goes.
struct Work {
    /// The node that sent it.
    from: NodeId,
    msg: Id,
    /// The requests in the message, see [`RemoteRequest`](super::RemoteRequest).
    requests: Vec<u64>,
    payload: Bytes,
    /// The number of the call, if the message is one, which `from` awaits the reply to.
    call_id: Option<u64>,
}

/// Runs while the node does: takes in what other nodes send, and notices when
/// they are lost.
pub(super) async fn serve(
    shared: Arc<Shared>,
    running: Started,
    mut inbox: mpsc::Receiver<Incoming>,
    mut peers: broadcast::Receiver<PeerEvent>,
    mut members: broadcast::Receiver<ClusterEvent>,
) {
    let mut router = Router {
        shared,
        started: running.clone(),
        routes: HashMap::new(),
    };
    let pending = running.pending;

    loop {
        tokio::select! {
            incoming = inbox.recv() => match incoming {
                Some(incoming) => router.on_incoming(incoming, &pending),
                None => break,
            },
            event = peers.recv() => match event {
                Ok(PeerEvent::Disconnected { node, .. }) => pending.fail_node(&node),
                Ok(_) => {}
                // Some reports were missed; calls that depended on them run out of time.
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    tracing::warn!("Missed reports about peers");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            event = members.recv() => match event {
                Ok(ClusterEvent::Left(member) | ClusterEvent::Failed(member)) => {
                    pending.fail_node(&member.node);
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    tracing::warn!("Missed changes in the cluster's members");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
}

/// Hands each message to the task of the actor it is for, so that one slow
/// actor doesn't hold up the others, and the messages for one are put in its
/// mailbox in the order they arrived.
struct Router {
    shared: Arc<Shared>,
    started: Started,
    routes: HashMap<Name, Route>,
}

impl Router {
    fn on_incoming(&mut self, incoming: Incoming, pending: &Pending) {
        let Some(frame) = Frame::decode(incoming.payload) else {
            tracing::debug!(node = %incoming.from, "Dropping a malformed message between actors");
            return;
        };
        match frame {
            Frame::Reply { call_id, result } => pending.complete(&incoming.from, call_id, result),
            Frame::Cast {
                target,
                msg,
                requests,
                payload,
            } => self.route(
                target,
                Work {
                    from: incoming.from,
                    msg,
                    requests,
                    payload,
                    call_id: None,
                },
            ),
            Frame::Call {
                call_id,
                target,
                msg,
                requests,
                payload,
            } => self.route(
                target,
                Work {
                    from: incoming.from,
                    msg,
                    requests,
                    payload,
                    call_id: Some(call_id),
                },
            ),
        }
    }

    fn route(&mut self, target: Name, work: Work) {
        let len = work.payload.len();
        let work = match self.routes.get(&target) {
            Some(route) => {
                if !route.queued.admit(len) {
                    self.refuse(work, RemoteError::Overloaded);
                    return;
                }
                match route.queue.send(work) {
                    Ok(()) => return,
                    // The task ended for lack of messages; start another.
                    Err(mpsc::error::SendError(work)) => {
                        route.queued.release(len);
                        work
                    }
                }
            }
            None => work,
        };

        // Don't keep the ends of tasks that are long gone.
        if self.routes.len() >= 1024 {
            self.routes.retain(|_, route| !route.queue.is_closed());
        }
        let (queue, tasks_queue) = mpsc::unbounded_channel();
        let queued = Arc::new(Queued::default());
        queued.admit(len);
        queue.send(work).expect("The task's queue is open");
        self.routes.insert(
            target.clone(),
            Route {
                queue,
                queued: queued.clone(),
            },
        );
        tokio::spawn(actor_task(
            self.shared.clone(),
            self.started.clone(),
            target,
            tasks_queue,
            queued,
        ));
    }

    /// Says no to a message without waiting.
    fn refuse(&self, work: Work, error: RemoteError) {
        let answers = work.call_id.into_iter().chain(work.requests);
        if work.call_id.is_none() {
            tracing::warn!("Dropping a message for an actor with too much waiting: {error}");
        }
        for call_id in answers {
            let Some(lane) = reply_lane(&self.shared, &self.started.links, &work.from, call_id)
            else {
                return;
            };
            let _ = lane.try_send(
                Frame::Reply {
                    call_id,
                    result: Err(error.clone()),
                }
                .encode(),
            );
        }
    }
}

/// The lane replies to `node` go through, if it is a member of the cluster.
fn reply_lane(
    shared: &Shared,
    links: &Links,
    node: &NodeId,
    call_id: u64,
) -> Option<mpsc::Sender<Bytes>> {
    let Some(member) = shared.cluster.member(node) else {
        tracing::debug!(%node, "Not replying to a node that is not a member");
        return None;
    };
    Some(links.sender(
        &member.node,
        &member.addr,
        Protocol::ACTORS,
        Delivery::Ordered,
        (call_id % SHARDS as u64) as u8,
    ))
}

/// Sends the reply to a call.
pub(super) async fn reply(
    shared: &Shared,
    links: &Links,
    node: &NodeId,
    call_id: u64,
    result: Result<Bytes, RemoteError>,
) {
    let Some(lane) = reply_lane(shared, links, node, call_id) else {
        return;
    };
    let mut frame = Frame::Reply { call_id, result }.encode();
    if frame.len() > MAX_MESSAGE_SIZE {
        frame = Frame::Reply {
            call_id,
            result: Err(RemoteError::TooLarge),
        }
        .encode();
    }
    let _ = lane.send(frame).await;
}

/// Delivers the messages for one actor, one after the other.
async fn actor_task(
    shared: Arc<Shared>,
    started: Started,
    name: Name,
    mut queue: mpsc::UnboundedReceiver<Work>,
    queued: Arc<Queued>,
) {
    loop {
        let work = match timeout(ACTOR_IDLE, queue.recv()).await {
            Ok(Some(work)) => work,
            Ok(None) => break,
            // Idle. Stop taking messages, and finish those that are already here.
            Err(_) => {
                queue.close();
                continue;
            }
        };

        queued.release(work.payload.len());
        let Work {
            from,
            msg,
            requests,
            payload,
            call_id,
        } = work;
        let wire = Wire::new(shared.clone(), started.clone(), from.clone());
        match deliver(&shared, wire, &name, msg, payload, call_id.is_some()).await {
            Ok(Some(waiting)) => {
                // Waiting for the actor to answer mustn't hold up the next message.
                let (shared, links) = (shared.clone(), started.links.clone());
                tokio::spawn(async move {
                    let result = waiting.await;
                    if let Some(call_id) = call_id {
                        reply(&shared, &links, &from, call_id, result).await;
                    }
                });
            }
            Ok(None) => {}
            Err(error) => {
                if call_id.is_none() {
                    tracing::debug!(%name, "Could not deliver a message that expected no reply: {error}");
                }
                // Whoever waits for an answer to this message learns it isn't coming.
                for id in call_id.into_iter().chain(requests) {
                    reply(&shared, &started.links, &from, id, Err(error.clone())).await;
                }
            }
        }
    }
}

async fn deliver(
    shared: &Shared,
    wire: Wire,
    name: &Name,
    msg: Id,
    payload: Bytes,
    reply: bool,
) -> Result<Option<ReplyFuture>, RemoteError> {
    let handler = shared
        .handlers
        .get(&msg)
        .map(|handler| handler.clone())
        .ok_or(RemoteError::UnknownMessage)?;
    let address = name.address().ok_or(RemoteError::NoSuchActor)?;
    handler.deliver(wire, address, payload, reply).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_queue_takes_bursts_and_refuses_sustained_overload() {
        let queued = Queued::default();
        // Many small messages are fine...
        for _ in 0..1_000 {
            assert!(queued.admit(100));
        }
        // ...until the size adds up.
        assert!(queued.admit(ACTOR_QUEUE_BYTES - 100_000));
        assert!(!queued.admit(1_000_000));

        // Room is made again as messages are taken.
        queued.release(ACTOR_QUEUE_BYTES - 100_000);
        assert!(queued.admit(1_000_000));
    }

    #[test]
    fn a_single_large_message_is_always_let_in() {
        let queued = Queued::default();
        assert!(queued.admit(2 * ACTOR_QUEUE_BYTES));
        assert!(!queued.admit(1));
        queued.release(2 * ACTOR_QUEUE_BYTES);
        assert!(queued.admit(1));
    }

    #[test]
    fn the_number_waiting_is_bounded_too() {
        let queued = Queued::default();
        for _ in 0..ACTOR_QUEUE_MESSAGES {
            assert!(queued.admit(0));
        }
        assert!(!queued.admit(0));
    }
}
