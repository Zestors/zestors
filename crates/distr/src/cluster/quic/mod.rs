//! A mutually authenticated message transport over QUIC.

mod frame;
mod hello;
mod peer;
mod tls;

#[cfg(test)]
mod tests;

pub use tls::{Tls, TlsError};

use super::net::{Event, Frame, Incoming, Net};
use crate::{ClusterTimings, Member, NodeId};
use bytes::Bytes;
use hello::{read_hello, send_hello};
use peer::peer_task;
use quinn::{Connection, Endpoint, IdleTimeout, SendStream, TransportConfig};
use std::{
    collections::HashMap,
    future::Future,
    io,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinSet, time::timeout};
use tokio_util::sync::{CancellationToken, DropGuard};

/// The largest message that is accepted from a peer.
const MAX_MESSAGE_SIZE: usize = 64 * 1024;
/// How many messages may be queued for a single peer before new ones are dropped.
const PEER_QUEUE: usize = 128;
/// How many connection attempts in a row must fail before a peer is reported
/// [`Event::Unreachable`].
const UNREACHABLE_AFTER: u32 = 3;

/// QUIC application close codes.
const CLOSE_SHUTDOWN: u32 = 0;
const CLOSE_DUPLICATE: u32 = 1;
const CLOSE_FORGOTTEN: u32 = 2;
const CLOSE_UNAUTHORIZED: u32 = 3;
const CLOSE_VERSION: u32 = 4;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

struct Outgoing {
    addr: SocketAddr,
    frame: Bytes,
    /// Send as a datagram if it fits, see [`Frame::is_datagram`].
    datagram: bool,
}

/// The one connection kept per peer.
struct Registered {
    conn: Connection,
    /// The peer's generation, from its [`Hello`].
    generation: u64,
    /// Which side opened the connection.
    dialer: NodeId,
}

struct Inner {
    endpoint: Endpoint,
    local: NodeId,
    generation: u64,
    timings: ClusterTimings,
    tls: Tls,
    conns: Mutex<HashMap<NodeId, Registered>>,
    peers: Mutex<HashMap<NodeId, mpsc::Sender<Outgoing>>>,
    peer_tasks: Mutex<JoinSet<()>>,
    events: mpsc::Sender<Event>,
    token: CancellationToken,
}

/// A mutually authenticated message transport between cluster nodes.
///
/// There is exactly one QUIC connection per pair of nodes, used in both
/// directions. When two nodes dial each other at the same time, both keep the
/// connection dialed by the node with the smaller name and close the other; a
/// peer that restarted (a higher generation) always replaces its old
/// connection.
///
/// Every message is one QUIC unidirectional stream. Delivery is best-effort:
/// messages to a slow or unreachable peer are dropped rather than queued
/// without bound, and sending never blocks the caller.
///
/// Dropping the transport stops all of its tasks.
pub(super) struct Transport {
    inner: Arc<Inner>,
    _cancel_on_drop: DropGuard,
}

impl Transport {
    /// Binds a QUIC endpoint on `bind` for the node `local` in `generation`,
    /// returning the transport together with the stream of messages received
    /// from peers.
    pub(super) fn bind(
        bind: SocketAddr,
        tls: &Tls,
        local: NodeId,
        generation: u64,
        timings: ClusterTimings,
    ) -> io::Result<(Self, mpsc::Receiver<Event>)> {
        let mut transport = TransportConfig::default();
        transport.keep_alive_interval(Some(timings.keep_alive));
        transport.max_idle_timeout(Some(
            IdleTimeout::try_from(timings.idle_timeout)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
        ));
        let transport = Arc::new(transport);

        let (server, client) = tls.quic_configs(transport)?;
        let mut endpoint = Endpoint::server(server, bind)?;
        endpoint.set_default_client_config(client);

        let (events_tx, events_rx) = mpsc::channel(1024);
        let token = CancellationToken::new();
        let inner = Arc::new(Inner {
            endpoint,
            local,
            generation,
            timings,
            tls: tls.clone(),
            conns: Mutex::new(HashMap::new()),
            peers: Mutex::new(HashMap::new()),
            peer_tasks: Mutex::new(JoinSet::new()),
            events: events_tx,
            token: token.clone(),
        });
        inner.spawn(inner.clone().accept_loop());

        Ok((
            Self {
                inner,
                _cancel_on_drop: token.drop_guard(),
            },
            events_rx,
        ))
    }

    /// The address the endpoint is bound to.
    pub(super) fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.endpoint.local_addr()
    }

    /// Delivers what is already queued, then closes all connections.
    pub(super) async fn shutdown(&self, grace: Duration) {
        // Dropping the senders lets every peer task finish its queue and exit.
        self.inner.peers.lock().expect("Not poisoned").clear();

        let mut tasks = std::mem::take(&mut *self.inner.peer_tasks.lock().expect("Not poisoned"));
        let _ = timeout(grace, async { while tasks.join_next().await.is_some() {} }).await;
        tasks.abort_all();

        self.inner
            .endpoint
            .close(CLOSE_SHUTDOWN.into(), b"shutdown");
        let _ = timeout(grace, self.inner.endpoint.wait_idle()).await;
        self.inner.token.cancel();
    }
}

impl Net for Transport {
    /// Queues `frame` for delivery to `to`.
    ///
    /// Never blocks. The message is dropped if the peer's queue is full.
    fn send(&self, to: &Member, frame: Frame) {
        self.inner.send(to, frame.encode(), frame.is_datagram());
    }

    /// Closes the connection to `node` if it belongs to that generation or an
    /// older one, so the next message dials afresh, and stops sending to it.
    /// Called when the node has been declared down or has moved.
    fn forget(&self, node: &NodeId, generation: u64) {
        // Ends the peer's sender task once it has sent what is queued, including
        // one that is backing off from an unreachable peer.
        self.inner.peers.lock().expect("Not poisoned").remove(node);

        let mut conns = self.inner.conns.lock().expect("Not poisoned");
        if conns
            .get(node)
            .is_some_and(|registered| registered.generation <= generation)
            && let Some(registered) = conns.remove(node)
        {
            registered.conn.close(CLOSE_FORGOTTEN.into(), b"forgotten");
        }
    }
}

impl Inner {
    /// Spawns `fut`, ending it when the transport is dropped or shut down.
    fn spawn(&self, fut: impl Future<Output = ()> + Send + 'static) {
        let token = self.token.clone();
        tokio::spawn(async move {
            let _ = token.run_until_cancelled_owned(fut).await;
        });
    }

    fn send(self: &Arc<Self>, to: &Member, frame: Bytes, datagram: bool) {
        let mut peers = self.peers.lock().expect("Not poisoned");
        let mut outgoing = Outgoing {
            addr: to.addr,
            frame,
            datagram,
        };

        if let Some(tx) = peers.get(&to.node) {
            match tx.try_send(outgoing) {
                Ok(()) => return,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    tracing::debug!(node = %to.node, "Peer queue full, dropping message");
                    return;
                }
                Err(mpsc::error::TrySendError::Closed(returned)) => {
                    // The peer task ended (idle); start a new one below.
                    outgoing = returned;
                    peers.remove(&to.node);
                }
            }
        }

        let (tx, rx) = mpsc::channel(PEER_QUEUE);
        tx.try_send(outgoing).expect("Fresh queue has capacity");
        peers.insert(to.node.clone(), tx);

        let token = self.token.clone();
        let task = peer_task(self.clone(), to.node.clone(), rx);
        self.peer_tasks
            .lock()
            .expect("Not poisoned")
            .spawn(async move {
                let _ = token.run_until_cancelled_owned(task).await;
            });
    }

    /// The live connection to `node`, if there is one.
    fn live(&self, node: &NodeId) -> Option<Connection> {
        let conns = self.conns.lock().expect("Not poisoned");
        conns
            .get(node)
            .filter(|registered| registered.conn.close_reason().is_none())
            .map(|registered| registered.conn.clone())
    }

    /// Makes `conn` the connection to `peer`, unless the one already there is
    /// preferred. Returns whether `conn` was kept.
    fn register(&self, peer: &NodeId, generation: u64, conn: &Connection, dialer: &NodeId) -> bool {
        let mut conns = self.conns.lock().expect("Not poisoned");

        if let Some(existing) = conns
            .get(peer)
            .filter(|existing| existing.conn.close_reason().is_none())
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
            existing
                .conn
                .close(CLOSE_DUPLICATE.into(), b"duplicate connection");
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

    fn unregister(&self, peer: &NodeId, stable_id: usize) {
        let mut conns = self.conns.lock().expect("Not poisoned");
        if conns
            .get(peer)
            .is_some_and(|registered| registered.conn.stable_id() == stable_id)
        {
            conns.remove(peer);
        }
    }

    /// Delivers everything the peer sends on `conn` until it closes.
    fn spawn_receive(self: &Arc<Self>, conn: Connection, peer: NodeId, generation: u64) {
        // Datagrams are handled inline: they are small and loss-tolerant, so a
        // full channel simply drops them rather than holding anything up.
        let datagrams = self.clone();
        let (datagram_conn, datagram_peer) = (conn.clone(), peer.clone());
        self.spawn(async move {
            while let Ok(bytes) = datagram_conn.read_datagram().await {
                if let Some(frame) = Frame::decode(bytes) {
                    let incoming = Incoming {
                        from: datagram_peer.clone(),
                        generation,
                        frame,
                    };
                    if datagrams
                        .events
                        .try_send(Event::Received(incoming))
                        .is_err()
                    {
                        tracing::debug!(node = %datagram_peer, "Inbox full, dropping datagram");
                    }
                }
            }
        });

        let inner = self.clone();
        self.spawn(async move {
            while let Ok(mut stream) = conn.accept_uni().await {
                let handler = inner.clone();
                let peer = peer.clone();
                inner.spawn(async move {
                    match stream.read_to_end(MAX_MESSAGE_SIZE).await {
                        Ok(bytes) => {
                            if let Some(frame) = Frame::decode(Bytes::from(bytes)) {
                                let _ = handler
                                    .events
                                    .send(Event::Received(Incoming {
                                        from: peer,
                                        generation,
                                        frame,
                                    }))
                                    .await;
                            }
                        }
                        Err(err) => tracing::debug!("Failed to read message: {err}"),
                    }
                });
            }
            inner.unregister(&peer, conn.stable_id());
        });
    }

    /// The connection to `node`, dialing `addr` if there is none yet.
    async fn connection(
        self: &Arc<Self>,
        node: &NodeId,
        addr: SocketAddr,
    ) -> Result<Connection, BoxError> {
        if let Some(conn) = self.live(node) {
            return Ok(conn);
        }

        let connecting = self.endpoint.connect(addr, node.as_str())?;
        let conn = timeout(self.timings.connect_timeout, connecting).await??;

        let hello = timeout(self.timings.handshake_timeout, async {
            send_hello(&conn, &self.local, self.generation).await?;
            read_hello(&conn).await
        })
        .await??;

        if hello.node != *node {
            conn.close(CLOSE_DUPLICATE.into(), b"unexpected node");
            return Err(format!("expected node {node}, found {}", hello.node).into());
        }
        self.authenticate(&conn, &hello.node)?;

        if self.register(&hello.node, hello.generation, &conn, &self.local) {
            self.spawn_receive(conn.clone(), hello.node, hello.generation);
            Ok(conn)
        } else {
            // The peer dialed us at the same time and that connection is preferred.
            conn.close(CLOSE_DUPLICATE.into(), b"duplicate connection");
            self.live(node)
                .ok_or_else(|| "connection was closed".into())
        }
    }

    /// Checks that the peer's certificate is valid for the node name it claims
    /// in its [`Hello`]. Mutual TLS only proves the peer holds *some*
    /// certificate from the cluster CA; without this any member could
    /// impersonate any other. Closes `conn` on failure.
    fn authenticate(&self, conn: &Connection, claimed: &NodeId) -> Result<(), BoxError> {
        if let Err(err) = self.tls.verify_peer(conn, claimed) {
            conn.close(CLOSE_UNAUTHORIZED.into(), b"node name not in certificate");
            return Err(format!("peer claiming to be {claimed} rejected: {err}").into());
        }
        Ok(())
    }

    async fn accept_loop(self: Arc<Self>) {
        while let Some(incoming) = self.endpoint.accept().await {
            let inner = self.clone();
            self.spawn(async move {
                if let Err(err) = inner.accept(incoming).await {
                    tracing::debug!("Incoming connection failed: {err}");
                }
            });
        }
    }

    async fn accept(self: &Arc<Self>, incoming: quinn::Incoming) -> Result<(), BoxError> {
        let conn = incoming.await?;

        let hello = timeout(self.timings.handshake_timeout, async {
            let hello = read_hello(&conn).await?;
            send_hello(&conn, &self.local, self.generation).await?;
            Ok::<_, BoxError>(hello)
        })
        .await??;
        self.authenticate(&conn, &hello.node)?;

        if self.register(&hello.node, hello.generation, &conn, &hello.node) {
            self.spawn_receive(conn, hello.node, hello.generation);
        } else {
            conn.close(CLOSE_DUPLICATE.into(), b"duplicate connection");
        }
        Ok(())
    }
}

async fn send_message(conn: &Connection, message: &[u8]) -> Result<SendStream, quinn::WriteError> {
    let mut stream = conn.open_uni().await?;
    stream.write_all(message).await?;
    stream
        .finish()
        .map_err(|_| quinn::WriteError::ClosedStream)?;
    Ok(stream)
}
