use crate::{ClusterTimings, Member, NodeId, Tls};
use bytes::{BufMut, Bytes, BytesMut};
use quinn::{
    ClientConfig, Connection, Endpoint, IdleTimeout, SendStream, ServerConfig, TransportConfig,
    crypto::rustls::{QuicClientConfig, QuicServerConfig},
};
use std::{
    collections::HashMap,
    future::Future,
    io,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::mpsc,
    task::JoinSet,
    time::{Instant, timeout},
};
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

/// A message between two nodes.
#[derive(Debug, Clone)]
pub(crate) enum Frame {
    /// Opaque membership protocol bytes.
    Gossip(Bytes),
    /// The sender is shutting down cleanly and is not going to come back.
    Departure,
}

/// The version of the connection protocol (the [`Hello`] exchange and [`Frame`]
/// encoding). Peers with a different version refuse each other, so bump it on
/// any incompatible change.
pub(crate) const PROTOCOL_VERSION: u16 = 1;

const KIND_HELLO: u8 = 0;
const KIND_GOSSIP: u8 = 1;
const KIND_DEPARTURE: u8 = 2;

impl Frame {
    /// Whether the frame may be sent as an unreliable datagram: loss-tolerant
    /// messages only. Everything else goes over a reliable stream.
    fn is_datagram(&self) -> bool {
        matches!(self, Frame::Gossip(_))
    }

    fn encode(&self) -> Bytes {
        match self {
            Frame::Gossip(data) => {
                let mut buf = BytesMut::with_capacity(1 + data.len());
                buf.put_u8(KIND_GOSSIP);
                buf.put_slice(data);
                buf.freeze()
            }
            Frame::Departure => Bytes::from_static(&[KIND_DEPARTURE]),
        }
    }

    fn decode(mut bytes: Bytes) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        match bytes.split_to(1)[0] {
            KIND_GOSSIP => Some(Frame::Gossip(bytes)),
            KIND_DEPARTURE => Some(Frame::Departure),
            _ => None,
        }
    }
}

/// Something the transport reports to the membership layer.
#[derive(Debug)]
pub(crate) enum Event {
    /// A message from a peer.
    Received(Incoming),
    /// Repeated attempts to connect to the peer have failed. It is retried with
    /// growing pauses in between until it answers.
    Unreachable(NodeId),
    /// The peer can be connected to again, after having been reported unreachable.
    Reachable(NodeId),
}

/// A [`Frame`] received from a peer whose identity was established when the
/// connection was set up.
#[derive(Debug)]
pub(crate) struct Incoming {
    pub(crate) from: NodeId,
    pub(crate) generation: u64,
    pub(crate) frame: Frame,
}

/// The first message on every connection, in both directions: who is on the
/// other end.
///
/// Layout: kind, protocol version (`u16`), generation (`u64`), node name. The
/// version comes first and is checked before anything after it is parsed, so
/// later versions are free to change the rest.
struct Hello {
    node: NodeId,
    generation: u64,
}

#[derive(Debug, thiserror::Error)]
enum HelloError {
    #[error("invalid hello")]
    Invalid,
    #[error("incompatible protocol version {0} (ours is {PROTOCOL_VERSION})")]
    Version(u16),
}

impl Hello {
    fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();
        buf.put_u8(KIND_HELLO);
        buf.put_u16(PROTOCOL_VERSION);
        buf.put_u64(self.generation);
        buf.put_slice(self.node.as_str().as_bytes());
        buf.freeze()
    }

    fn decode(bytes: &[u8]) -> Result<Self, HelloError> {
        let (&kind, rest) = bytes.split_first().ok_or(HelloError::Invalid)?;
        if kind != KIND_HELLO || rest.len() < 2 {
            return Err(HelloError::Invalid);
        }
        let (version, rest) = rest.split_at(2);
        let version = u16::from_be_bytes([version[0], version[1]]);
        if version != PROTOCOL_VERSION {
            return Err(HelloError::Version(version));
        }
        if rest.len() < 8 {
            return Err(HelloError::Invalid);
        }
        let (generation, node) = rest.split_at(8);
        Ok(Self {
            node: NodeId::new(std::str::from_utf8(node).map_err(|_| HelloError::Invalid)?),
            generation: u64::from_be_bytes(generation.try_into().unwrap()),
        })
    }
}

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
    verify_names: bool,
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
pub(crate) struct Transport {
    inner: Arc<Inner>,
    _cancel_on_drop: DropGuard,
}

impl Transport {
    /// Binds a QUIC endpoint on `bind` for the node `local` in `generation`,
    /// returning the transport together with the stream of messages received
    /// from peers.
    pub(crate) fn bind(
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

        let server_crypto = QuicServerConfig::try_from(tls.server.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let mut server = ServerConfig::with_crypto(Arc::new(server_crypto));
        server.transport_config(transport.clone());

        let client_crypto = QuicClientConfig::try_from(tls.client.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let mut client = ClientConfig::new(Arc::new(client_crypto));
        client.transport_config(transport);

        let mut endpoint = Endpoint::server(server, bind)?;
        endpoint.set_default_client_config(client);

        let (events_tx, events_rx) = mpsc::channel(1024);
        let token = CancellationToken::new();
        let inner = Arc::new(Inner {
            endpoint,
            local,
            generation,
            timings,
            verify_names: tls.verify_names,
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
    pub(crate) fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.endpoint.local_addr()
    }

    /// Queues `frame` for delivery to `to`.
    ///
    /// Never blocks. The message is dropped if the peer's queue is full.
    pub(crate) fn send(&self, to: &Member, frame: Frame) {
        self.inner.send(to, frame.encode(), frame.is_datagram());
    }

    /// Closes the connection to `node` if it belongs to that generation or an
    /// older one, so the next message dials afresh, and stops sending to it.
    /// Called when the node has been declared down or has moved.
    pub(crate) fn forget(&self, node: &NodeId, generation: u64) {
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

    /// Delivers what is already queued, then closes all connections.
    pub(crate) async fn shutdown(&self, grace: Duration) {
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
        if !self.verify_names {
            return Ok(());
        }
        if let Err(err) = certificate_matches(conn, claimed) {
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

async fn peer_task(inner: Arc<Inner>, node: NodeId, mut rx: mpsc::Receiver<Outgoing>) {
    let mut last: Option<SendStream> = None;
    // Consecutive failed attempts to connect, and when the next one may happen.
    let mut failures: u32 = 0;
    let mut retry_at: Option<Instant> = None;
    let mut reported_unreachable = false;

    loop {
        let message = match timeout(inner.timings.peer_idle, rx.recv()).await {
            Ok(Some(message)) => message,
            Ok(None) => break,
            // Idle. Keep going for a peer reported unreachable, so that the
            // report is always followed by a matching `Reachable`; the task
            // ends when the peer is forgotten.
            Err(_) if reported_unreachable => continue,
            Err(_) => break,
        };

        if retry_at.is_some_and(|at| Instant::now() < at) {
            tracing::trace!(%node, "Backing off, dropping message");
            continue;
        }

        let conn = match inner.connection(&node, message.addr).await {
            Ok(conn) => {
                if failures > 0 {
                    tracing::debug!(%node, failures, "Peer is reachable again");
                }
                failures = 0;
                retry_at = None;
                if reported_unreachable {
                    reported_unreachable = false;
                    let _ = inner.events.send(Event::Reachable(node.clone())).await;
                }
                conn
            }
            Err(err) => {
                failures = failures.saturating_add(1);
                let backoff = backoff(&inner.timings, failures);
                retry_at = Some(Instant::now() + backoff);
                tracing::debug!(
                    %node, addr = %message.addr, failures, ?backoff,
                    "Failed to connect to peer: {err}"
                );
                if failures >= UNREACHABLE_AFTER && !reported_unreachable {
                    reported_unreachable = true;
                    let _ = inner.events.send(Event::Unreachable(node.clone())).await;
                }
                continue;
            }
        };

        if message.datagram {
            match conn.send_datagram(message.frame.clone()) {
                Ok(()) => continue,
                // Doesn't fit in a packet, or the peer takes no datagrams: a
                // stream delivers it just the same.
                Err(
                    quinn::SendDatagramError::TooLarge
                    | quinn::SendDatagramError::UnsupportedByPeer
                    | quinn::SendDatagramError::Disabled,
                ) => {}
                Err(err) => {
                    tracing::debug!(%node, "Failed to send datagram to peer: {err}");
                    inner.unregister(&node, conn.stable_id());
                    continue;
                }
            }
        }

        match send_message(&conn, &message.frame).await {
            Ok(stream) => last = Some(stream),
            Err(err) => {
                tracing::debug!(%node, "Failed to send to peer: {err}");
                inner.unregister(&node, conn.stable_id());
            }
        }
    }

    // Give the final message a moment to be acknowledged.
    if let Some(stream) = last {
        let _ = timeout(Duration::from_secs(1), stream.stopped()).await;
    }
}

/// How long to wait after the `failures`-th failed connection attempt in a row:
/// doubling from [`ClusterTimings::reconnect_backoff_min`] up to
/// [`ClusterTimings::reconnect_backoff_max`], with jitter so peers that lost
/// the same node don't all retry in lockstep.
fn backoff(timings: &ClusterTimings, failures: u32) -> Duration {
    let exponent = failures.saturating_sub(1).min(16);
    let base = timings
        .reconnect_backoff_min
        .saturating_mul(1 << exponent)
        .min(timings.reconnect_backoff_max);
    base.mul_f64(rand::random_range(0.75..1.25))
}

async fn send_message(conn: &Connection, message: &[u8]) -> Result<SendStream, quinn::WriteError> {
    let mut stream = conn.open_uni().await?;
    stream.write_all(message).await?;
    stream
        .finish()
        .map_err(|_| quinn::WriteError::ClosedStream)?;
    Ok(stream)
}

async fn send_hello(conn: &Connection, node: &NodeId, generation: u64) -> Result<(), BoxError> {
    let hello = Hello {
        node: node.clone(),
        generation,
    };
    send_message(conn, &hello.encode()).await?;
    Ok(())
}

async fn read_hello(conn: &Connection) -> Result<Hello, BoxError> {
    let mut stream = conn.accept_uni().await?;
    let bytes = stream.read_to_end(1024).await?;
    match Hello::decode(&bytes) {
        Ok(hello) => Ok(hello),
        Err(err) => {
            if matches!(err, HelloError::Version(_)) {
                conn.close(CLOSE_VERSION.into(), b"incompatible protocol version");
            }
            Err(err.into())
        }
    }
}

/// Whether the certificate `conn`'s peer presented is valid for the DNS name `node`.
fn certificate_matches(conn: &Connection, node: &NodeId) -> Result<(), BoxError> {
    let identity = conn
        .peer_identity()
        .ok_or("peer presented no certificate")?;
    let chain = identity
        .downcast::<Vec<quinn::rustls::pki_types::CertificateDer<'static>>>()
        .map_err(|_| "unexpected peer identity type")?;
    let end_entity = chain.first().ok_or("empty certificate chain")?;
    let name = quinn::rustls::pki_types::ServerName::try_from(node.as_str())?;
    webpki::EndEntityCert::try_from(end_entity)?.verify_is_valid_for_subject_name(&name)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_round_trips() {
        let hello = Hello {
            node: NodeId::new("node-a"),
            generation: 42,
        };
        let decoded = Hello::decode(&hello.encode()).unwrap();
        assert_eq!(decoded.node, hello.node);
        assert_eq!(decoded.generation, 42);
    }

    #[test]
    fn hello_with_other_version_is_refused_before_parsing_the_rest() {
        // A future version may lay out the remainder differently: even an
        // otherwise unparseable body must be reported as a version mismatch.
        let mut bytes = vec![KIND_HELLO];
        bytes.extend_from_slice(&(PROTOCOL_VERSION + 1).to_be_bytes());
        assert!(matches!(
            Hello::decode(&bytes),
            Err(HelloError::Version(v)) if v == PROTOCOL_VERSION + 1
        ));
    }

    #[test]
    fn malformed_hellos_are_refused() {
        assert!(matches!(Hello::decode(&[]), Err(HelloError::Invalid)));
        assert!(matches!(
            Hello::decode(&[KIND_GOSSIP, 0, 1]),
            Err(HelloError::Invalid)
        ));
        let mut truncated = vec![KIND_HELLO];
        truncated.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
        truncated.extend_from_slice(&[0; 4]);
        assert!(matches!(
            Hello::decode(&truncated),
            Err(HelloError::Invalid)
        ));
    }

    fn bind(name: &str) -> (Transport, mpsc::Receiver<Event>, Member) {
        bind_at(
            name,
            "127.0.0.1:0".parse().unwrap(),
            ClusterTimings::default(),
        )
    }

    fn bind_at(
        name: &str,
        addr: SocketAddr,
        timings: ClusterTimings,
    ) -> (Transport, mpsc::Receiver<Event>, Member) {
        let (transport, events) = Transport::bind(
            addr,
            &Tls::insecure_dev().unwrap(),
            NodeId::new(name),
            1,
            timings,
        )
        .unwrap();
        let member = Member {
            node: NodeId::new(name),
            addr: transport.local_addr().unwrap(),
            generation: 1,
        };
        (transport, events, member)
    }

    async fn next_gossip(events: &mut mpsc::Receiver<Event>) -> Bytes {
        loop {
            let event = timeout(Duration::from_secs(5), events.recv())
                .await
                .expect("message arrives")
                .expect("transport is open");
            match event {
                Event::Received(Incoming {
                    frame: Frame::Gossip(data),
                    ..
                }) => return data,
                Event::Received(other) => panic!("expected gossip, got {other:?}"),
                Event::Unreachable(_) | Event::Reachable(_) => {}
            }
        }
    }

    #[tokio::test]
    async fn gossip_is_delivered_as_datagram_or_stream_fallback() {
        let (a, _a_events, _) = bind("node-a");
        let (_b, mut b_events, b_member) = bind("node-b");

        // Small: fits in a datagram.
        let small = Bytes::from(vec![7u8; 100]);
        a.send(&b_member, Frame::Gossip(small.clone()));
        assert_eq!(next_gossip(&mut b_events).await, small);
        let conn = a.inner.live(&b_member.node).expect("connected to b");
        assert_eq!(conn.stats().frame_tx.datagram, 1, "sent as a datagram");

        // Too big for any datagram: falls back to a stream.
        let large = Bytes::from(vec![9u8; 20_000]);
        a.send(&b_member, Frame::Gossip(large.clone()));
        assert_eq!(next_gossip(&mut b_events).await, large);
        assert_eq!(
            conn.stats().frame_tx.datagram,
            1,
            "the large message did not go out as a datagram"
        );
    }

    #[tokio::test]
    async fn unreachable_peer_is_reported_and_reported_reachable_when_it_answers() {
        let timings = ClusterTimings {
            connect_timeout: Duration::from_millis(100),
            reconnect_backoff_min: Duration::from_millis(20),
            reconnect_backoff_max: Duration::from_millis(100),
            ..ClusterTimings::default()
        };
        let (a, mut a_events, _) =
            bind_at("node-a", "127.0.0.1:0".parse().unwrap(), timings.clone());

        // An address nobody listens on yet.
        let addr = std::net::UdpSocket::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let b_member = Member {
            node: NodeId::new("node-b"),
            addr,
            generation: 1,
        };

        // Keep trying to reach it; messages sent while backing off are dropped.
        async fn until(
            a: &Transport,
            to: &Member,
            events: &mut mpsc::Receiver<Event>,
            wanted: impl Fn(&Event) -> bool,
        ) {
            timeout(Duration::from_secs(10), async {
                let mut tick = tokio::time::interval(Duration::from_millis(20));
                loop {
                    tokio::select! {
                        _ = tick.tick() => a.send(to, Frame::Gossip(Bytes::from_static(b"hi"))),
                        Some(event) = events.recv() => if wanted(&event) { return },
                    }
                }
            })
            .await
            .expect("expected event");
        }

        until(
            &a,
            &b_member,
            &mut a_events,
            |e| matches!(e, Event::Unreachable(n) if n.as_str() == "node-b"),
        )
        .await;

        // The peer comes up at that address.
        let (_b, _b_events, _) = bind_at("node-b", addr, timings);
        until(
            &a,
            &b_member,
            &mut a_events,
            |e| matches!(e, Event::Reachable(n) if n.as_str() == "node-b"),
        )
        .await;
    }
}
