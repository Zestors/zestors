//! Messages between nodes, on top of what a [backend](crate::backend)
//! provides.
//!
//! [`Links`] is what the layers of a node send and receive messages through.
//! It keeps one connection per pair of nodes, dials on demand and backs off
//! while a peer is unreachable, and reports [`PeerEvent`]s. Messages go into a
//! bounded pipe per peer, [`Protocol`] and [`Delivery`]; each ordered pipe is a
//! long-lived stream, so messages on it arrive in order, while the pipes to one
//! peer don't hold each other up. Messages that arrive are routed to whoever
//! subscribed to their protocol.
//!
//! All of it is the same for every backend, which only has to provide
//! connections.

mod dynamic;
mod lane;
mod wire;

#[cfg(all(test, feature = "quic"))]
mod tests;

use crate::{Addr, ClusterTimings, Member, NodeId, backend::LocalNode};
use bytes::Bytes;
use dynamic::{DynConnection, DynEndpoint, ErasedBackend};
use lane::{Health, lane};
use std::{
    collections::HashMap,
    io,
    ops::Deref,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::{io::AsyncReadExt, sync::mpsc, task::JoinSet, time::timeout};
use tokio_util::sync::{CancellationToken, DropGuard};
use wire::{Hello, MAX_MESSAGE_SIZE, read_frame, read_hello, write_hello};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// How many messages may be queued for one sender before it has to wait, or,
/// for datagrams, before new ones are dropped.
const LANE_QUEUE: usize = 128;
/// How many messages may wait for a subscriber to take them.
const INBOX_QUEUE: usize = 1024;

/// Which layer of a node a message is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct Protocol(u8);

impl Protocol {
    /// Cluster membership.
    pub(super) const MEMBERSHIP: Protocol = Protocol(0);
}

/// How reliably a message must arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Delivery {
    /// May be lost, and may overtake other messages. For traffic that repeats
    /// itself anyway, such as gossip.
    Datagram,
    /// Arrives complete and in order relative to what else is sent on the same
    /// pipe, or not at all if the connection is lost.
    Ordered,
}

/// A message from a peer, whose identity was established when the connection
/// was set up.
#[derive(Debug)]
pub(super) struct Incoming {
    pub(super) from: NodeId,
    pub(super) generation: u64,
    pub(super) payload: Bytes,
}

/// A change in how a node's peers can be reached.
#[derive(Debug)]
pub(super) enum PeerEvent {
    /// Repeated attempts to reach the peer have failed. It is retried with
    /// growing pauses in between until it answers.
    Unreachable(NodeId),
    /// The peer can be reached again, after having been reported unreachable.
    Reachable(NodeId),
    /// An established connection to the peer was lost or replaced. Messages
    /// sent on it may not have arrived.
    Disconnected { node: NodeId, generation: u64 },
}

/// A connection, told apart from the ones before and after it.
#[derive(Clone)]
struct Conn {
    id: u64,
    conn: Arc<dyn DynConnection>,
}

impl Deref for Conn {
    type Target = dyn DynConnection;

    fn deref(&self) -> &Self::Target {
        &*self.conn
    }
}

/// The one connection kept per peer.
struct Registered {
    conn: Conn,
    /// The peer's generation, from its [`Hello`].
    generation: u64,
    /// Which side opened the connection.
    dialer: NodeId,
}

/// A pipe to a peer for one protocol and delivery.
struct Lane {
    tx: mpsc::Sender<Bytes>,
    addr: Addr,
}

type LaneKey = (NodeId, Protocol, Delivery);

struct Inner {
    endpoint: Box<dyn DynEndpoint>,
    local: NodeId,
    generation: u64,
    timings: ClusterTimings,
    next_conn: AtomicU64,
    conns: Mutex<HashMap<NodeId, Registered>>,
    /// Held while dialing a peer, so that its lanes share one connection
    /// instead of dialing at once and closing each other's.
    dialing: Mutex<HashMap<NodeId, Arc<tokio::sync::Mutex<()>>>>,
    lanes: Mutex<HashMap<LaneKey, Lane>>,
    /// How reaching each peer is going, shared by all lanes to it.
    health: Mutex<HashMap<NodeId, Arc<Mutex<Health>>>>,
    inboxes: Mutex<HashMap<Protocol, mpsc::Sender<Incoming>>>,
    lane_tasks: Mutex<JoinSet<()>>,
    peer_events: mpsc::Sender<PeerEvent>,
    token: CancellationToken,
}

/// A backend, not yet started.
pub(super) struct Starter(Box<dyn ErasedBackend>);

impl Starter {
    pub(super) fn new(backend: impl crate::backend::Backend) -> Self {
        Self(Box::new(backend))
    }

    /// Starts the backend for `local`. Returns the links, the address peers
    /// reach the node on, and the stream of reports about peers.
    pub(super) async fn start(
        self,
        local: LocalNode,
        timings: ClusterTimings,
    ) -> io::Result<(Links, Addr, mpsc::Receiver<PeerEvent>)> {
        let (id, generation) = (local.id.clone(), local.generation);
        let endpoint = self.0.start(local).await?;
        let addr = endpoint.local_addr()?;

        let (peer_events, peer_events_rx) = mpsc::channel(256);
        let token = CancellationToken::new();
        let inner = Arc::new(Inner {
            endpoint,
            local: id,
            generation,
            timings,
            next_conn: AtomicU64::new(0),
            conns: Mutex::new(HashMap::new()),
            dialing: Mutex::new(HashMap::new()),
            lanes: Mutex::new(HashMap::new()),
            health: Mutex::new(HashMap::new()),
            inboxes: Mutex::new(HashMap::new()),
            lane_tasks: Mutex::new(JoinSet::new()),
            peer_events,
            token: token.clone(),
        });
        inner.spawn(inner.clone().accept_loop());

        let links = Links {
            inner,
            _cancel_on_drop: Arc::new(token.drop_guard()),
        };
        Ok((links, addr, peer_events_rx))
    }
}

/// What the layers of a node send and receive messages through, see the
/// [module docs](self).
///
/// Cheap to clone. Dropping the last clone stops all of its tasks.
#[derive(Clone)]
pub(super) struct Links {
    inner: Arc<Inner>,
    _cancel_on_drop: Arc<DropGuard>,
}

impl Links {
    /// A pipe to `to` for one protocol, which its messages are sent into.
    ///
    /// Cheap to call, and the sender is cheap to clone: clones of one pipe per
    /// peer, protocol and delivery are handed out. The channel is bounded:
    /// awaiting `send` waits for room, for [`Delivery::Ordered`]; `try_send`
    /// fails when full, which suits [`Delivery::Datagram`]. Messages are at most
    /// [`MAX_MESSAGE_SIZE`] bytes. The channel closes when the peer is forgotten
    /// or the links shut down; callers that find it closed ask for a new one.
    pub(super) fn sender(
        &self,
        to: &Member,
        protocol: Protocol,
        delivery: Delivery,
    ) -> mpsc::Sender<Bytes> {
        self.inner.sender(to, protocol, delivery)
    }

    /// The messages that arrive for `protocol`. Messages for a protocol nobody
    /// subscribed to are dropped, so subscribe before there is a chance of
    /// hearing from peers. A second subscription replaces the first.
    pub(super) fn subscribe(&self, protocol: Protocol) -> mpsc::Receiver<Incoming> {
        let (tx, rx) = mpsc::channel(INBOX_QUEUE);
        self.inner
            .inboxes
            .lock()
            .expect("Not poisoned")
            .insert(protocol, tx);
        rx
    }

    /// Closes the connection to `node` if it belongs to that generation or an
    /// older one, so the next message dials afresh, and stops sending to it.
    /// Called when the node has been declared down or has moved.
    pub(super) fn forget(&self, node: &NodeId, generation: u64) {
        // Ends the peer's lanes once they have sent what is queued, including
        // ones that are backing off from an unreachable peer.
        self.inner
            .lanes
            .lock()
            .expect("Not poisoned")
            .retain(|(lane_node, _, _), _| lane_node != node);
        self.inner.health.lock().expect("Not poisoned").remove(node);

        let mut conns = self.inner.conns.lock().expect("Not poisoned");
        if conns
            .get(node)
            .is_some_and(|registered| registered.generation <= generation)
            && let Some(registered) = conns.remove(node)
        {
            registered.conn.conn.close();
        }
    }

    /// Delivers what is already queued, waiting at most `grace`, then closes
    /// every connection.
    pub(super) async fn shutdown(&self, grace: std::time::Duration) {
        // Dropping our ends of the pipes lets every lane finish its queue and exit.
        self.inner.lanes.lock().expect("Not poisoned").clear();

        let mut tasks = std::mem::take(&mut *self.inner.lane_tasks.lock().expect("Not poisoned"));
        let _ = timeout(grace, async { while tasks.join_next().await.is_some() {} }).await;
        tasks.abort_all();

        self.inner.endpoint.close(grace).await;
        self.inner.token.cancel();
    }
}

impl Inner {
    /// Spawns `fut`, ending it when the links are dropped or shut down.
    fn spawn(&self, fut: impl Future<Output = ()> + Send + 'static) {
        let token = self.token.clone();
        tokio::spawn(async move {
            let _ = token.run_until_cancelled_owned(fut).await;
        });
    }

    /// The pipe to `to` for `protocol` and `delivery`, started if there is none.
    fn sender(
        self: &Arc<Self>,
        to: &Member,
        protocol: Protocol,
        delivery: Delivery,
    ) -> mpsc::Sender<Bytes> {
        let key = (to.node.clone(), protocol, delivery);
        let mut lanes = self.lanes.lock().expect("Not poisoned");

        // The pipe ends when idle; one that leads to where the node used to be is stale.
        if let Some(lane) = lanes.get(&key)
            && lane.addr == to.addr
            && !lane.tx.is_closed()
        {
            return lane.tx.clone();
        }

        let (tx, rx) = mpsc::channel(LANE_QUEUE);
        let health = self
            .health
            .lock()
            .expect("Not poisoned")
            .entry(to.node.clone())
            .or_default()
            .clone();
        lanes.insert(
            key,
            Lane {
                tx: tx.clone(),
                addr: to.addr.clone(),
            },
        );

        let token = self.token.clone();
        let task = lane(
            self.clone(),
            to.node.clone(),
            to.addr.clone(),
            protocol,
            delivery,
            health,
            rx,
        );
        self.lane_tasks
            .lock()
            .expect("Not poisoned")
            .spawn(async move {
                let _ = token.run_until_cancelled_owned(task).await;
            });
        tx
    }

    /// The live connection to `node`, if there is one.
    fn live(&self, node: &NodeId) -> Option<Conn> {
        let conns = self.conns.lock().expect("Not poisoned");
        conns
            .get(node)
            .filter(|registered| !registered.conn.is_closed())
            .map(|registered| registered.conn.clone())
    }

    /// Makes `conn` the connection to `peer`, unless the one already there is
    /// preferred. Returns whether `conn` was kept.
    fn register(&self, peer: &NodeId, generation: u64, conn: &Conn, dialer: &NodeId) -> bool {
        let mut conns = self.conns.lock().expect("Not poisoned");

        if let Some(existing) = conns
            .get(peer)
            .filter(|existing| !existing.conn.is_closed())
        {
            let keep_new = match generation.cmp(&existing.generation) {
                // The peer restarted, or this is an outdated connection.
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Less => false,
                std::cmp::Ordering::Equal if *dialer == existing.dialer => true,
                // Both sides dialed at once: keep the one dialed by the smaller name.
                std::cmp::Ordering::Equal => {
                    let smaller = if self.local <= *peer {
                        &self.local
                    } else {
                        peer
                    };
                    dialer == smaller
                }
            };
            if !keep_new {
                return false;
            }
            existing.conn.close();
            // Whatever was in flight on it may be lost.
            let _ = self.peer_events.try_send(PeerEvent::Disconnected {
                node: peer.clone(),
                generation: existing.generation,
            });
        }

        conns.insert(
            peer.clone(),
            Registered {
                conn: conn.clone(),
                generation,
                dialer: dialer.clone(),
            },
        );
        true
    }

    /// Forgets the connection `id` if it is still the connection to `peer`, and
    /// says so. One that was replaced or forgotten on purpose is not reported.
    fn unregister(&self, peer: &NodeId, id: u64) {
        let mut conns = self.conns.lock().expect("Not poisoned");
        if conns
            .get(peer)
            .is_some_and(|registered| registered.conn.id == id)
            && let Some(registered) = conns.remove(peer)
        {
            let _ = self.peer_events.try_send(PeerEvent::Disconnected {
                node: peer.clone(),
                generation: registered.generation,
            });
        }
    }

    /// Hands a message from `peer` to whoever subscribed to its protocol.
    /// Waits for room in their queue if `wait`, and drops the message otherwise.
    async fn deliver(
        &self,
        peer: &NodeId,
        generation: u64,
        protocol: Protocol,
        payload: Bytes,
        wait: bool,
    ) {
        let inbox = self
            .inboxes
            .lock()
            .expect("Not poisoned")
            .get(&protocol)
            .cloned();
        let Some(inbox) = inbox else {
            tracing::debug!(node = %peer, protocol = protocol.0, "Nobody subscribed, dropping message");
            return;
        };
        let incoming = Incoming {
            from: peer.clone(),
            generation,
            payload,
        };
        if wait {
            let _ = inbox.send(incoming).await;
        } else if inbox.try_send(incoming).is_err() {
            tracing::debug!(node = %peer, protocol = protocol.0, "Inbox full, dropping message");
        }
    }

    /// Delivers everything the peer sends on `conn` until it closes.
    fn spawn_receive(self: &Arc<Self>, conn: Conn, peer: NodeId, generation: u64) {
        // Datagrams: `[protocol][message]`. They are loss-tolerant, so a full
        // inbox simply drops them rather than holding anything up.
        let inner = self.clone();
        let (datagram_conn, datagram_peer) = (conn.clone(), peer.clone());
        self.spawn(async move {
            while let Ok(mut bytes) = datagram_conn.recv_datagram().await {
                if bytes.is_empty() {
                    continue;
                }
                let protocol = Protocol(bytes.split_to(1)[0]);
                inner
                    .deliver(&datagram_peer, generation, protocol, bytes, false)
                    .await;
            }
        });

        // Streams: `[protocol]`, then any number of frames.
        let inner = self.clone();
        self.spawn(async move {
            while let Ok((send, recv)) = conn.accept_stream().await {
                let handler = inner.clone();
                let peer = peer.clone();
                inner.spawn(async move {
                    if let Err(err) = handler.read_stream(send, recv, &peer, generation).await {
                        tracing::debug!(node = %peer, "Failed to read from peer: {err}");
                    }
                });
            }
            inner.unregister(&peer, conn.id);
        });
    }

    /// Reads the messages on one stream, in order, until it ends.
    async fn read_stream(
        &self,
        // Ended when this is dropped, which tells the sender that everything
        // it wrote has been read.
        _send: crate::backend::SendStream,
        mut recv: crate::backend::RecvStream,
        peer: &NodeId,
        generation: u64,
    ) -> io::Result<()> {
        let mut protocol = [0u8; 1];
        if recv.read(&mut protocol).await? == 0 {
            return Ok(());
        }
        let protocol = Protocol(protocol[0]);

        while let Some(payload) = read_frame(&mut recv, MAX_MESSAGE_SIZE).await? {
            // Waiting here holds up this stream and, through the backend's
            // flow control, its sender: what ordered delivery is meant to do.
            self.deliver(peer, generation, protocol, payload, true)
                .await;
        }
        Ok(())
    }

    /// The connection to `node`, dialing `addr` if there is none yet.
    async fn connection(self: &Arc<Self>, node: &NodeId, addr: &Addr) -> Result<Conn, BoxError> {
        if let Some(conn) = self.live(node) {
            return Ok(conn);
        }

        // One lane dials; the others wait for it and then find its connection.
        let dialing = self
            .dialing
            .lock()
            .expect("Not poisoned")
            .entry(node.clone())
            .or_default()
            .clone();
        let _dialing = dialing.lock().await;
        if let Some(conn) = self.live(node) {
            return Ok(conn);
        }

        let conn = timeout(
            self.timings.connect_timeout,
            self.endpoint.connect(addr, node),
        )
        .await??;
        let conn = self.conn(conn);

        let hello = timeout(self.timings.handshake_timeout, async {
            let (mut send, mut recv) = conn.open_stream().await?;
            write_hello(
                &mut send,
                &Hello {
                    node: self.local.clone(),
                    generation: self.generation,
                },
            )
            .await?;
            read_hello(&mut recv).await
        })
        .await??;

        if hello.node != *node {
            conn.close();
            return Err(format!("expected node {node}, found {}", hello.node).into());
        }
        self.authenticate(&conn, &hello.node)?;

        if self.register(&hello.node, hello.generation, &conn, &self.local) {
            self.spawn_receive(conn.clone(), hello.node, hello.generation);
            Ok(conn)
        } else {
            // The peer dialed us at the same time and that connection is preferred.
            conn.close();
            self.live(node)
                .ok_or_else(|| "connection was closed".into())
        }
    }

    fn conn(&self, conn: Arc<dyn DynConnection>) -> Conn {
        Conn {
            id: self.next_conn.fetch_add(1, Ordering::Relaxed),
            conn,
        }
    }

    /// Checks that the peer may claim the node name it gave in its [`Hello`].
    /// Closes `conn` if not.
    fn authenticate(&self, conn: &Conn, claimed: &NodeId) -> Result<(), BoxError> {
        if !conn.authenticates(claimed) {
            conn.close();
            return Err(format!("peer claiming to be {claimed} rejected").into());
        }
        Ok(())
    }

    async fn accept_loop(self: Arc<Self>) {
        while let Ok(conn) = self.endpoint.accept().await {
            let inner = self.clone();
            let conn = self.conn(conn);
            self.spawn(async move {
                if let Err(err) = inner.accept(conn).await {
                    tracing::debug!("Incoming connection failed: {err}");
                }
            });
        }
    }

    async fn accept(self: &Arc<Self>, conn: Conn) -> Result<(), BoxError> {
        let hello = timeout(self.timings.handshake_timeout, async {
            let (mut send, mut recv) = conn.accept_stream().await?;
            let hello = read_hello(&mut recv).await?;
            write_hello(
                &mut send,
                &Hello {
                    node: self.local.clone(),
                    generation: self.generation,
                },
            )
            .await?;
            Ok::<_, BoxError>(hello)
        })
        .await??;
        self.authenticate(&conn, &hello.node)?;

        if self.register(&hello.node, hello.generation, &conn, &hello.node) {
            self.spawn_receive(conn, hello.node, hello.generation);
        } else {
            conn.close();
        }
        Ok(())
    }
}
