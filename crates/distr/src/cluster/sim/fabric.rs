//! The in-memory network behind [`SimNetwork`](super::SimNetwork).
//!
//! A [`Fabric`] is a network that any number of simulated nodes are bound to.
//! Its endpoints and connections implement the same [`Endpoint`] and
//! [`Connection`] traits as a real backend does, so the real
//! cluster code runs on top of them unchanged. Bytes travel through pipes that
//! hold them back for the link's latency, so under
//! `#[tokio::test(start_paused = true)]` a whole cluster runs on virtual time.
//!
//! The fabric mirrors what a backend promises and nothing more: connections
//! between named nodes, ordered streams that lose nothing, and datagrams that
//! may be lost or overtake each other. It adds what real networks do to tests:
//! latency, and partitions, which cut every connection across them.

use crate::{
    NodeAddr, NodeName,
    backend::{Connection, DatagramError, Endpoint, RecvStream, SendStream},
};
use bytes::{Bytes, BytesMut};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use std::{
    collections::{HashMap, HashSet},
    io,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, DuplexStream},
    sync::mpsc,
    time::{Instant, sleep_until},
};
use tokio_util::sync::CancellationToken;

/// The largest datagram, like the MTU allows on a real network.
const MAX_DATAGRAM: usize = 1200;
const PIPE_BUFFER: usize = 64 * 1024;
const INCOMING_QUEUE: usize = 64;

/// A simulated network.
#[derive(Clone)]
pub(super) struct Fabric {
    state: Arc<Mutex<State>>,
}

struct State {
    /// The nodes currently listening, by the address they listen on.
    listeners: HashMap<NodeAddr, Listener>,
    /// Pairs of nodes that can't reach each other, in both directions.
    partitions: HashSet<(NodeName, NodeName)>,
    /// Every connection made, to cut the ones a partition falls across.
    pairs: Vec<(NodeName, NodeName, Weak<Pair>)>,
    latency: Duration,
    jitter: Duration,
    rng: StdRng,
    next_instance: u64,
}

struct Listener {
    node: NodeName,
    /// Tells a stale registration from the one that replaced it.
    instance: u64,
    incoming: mpsc::Sender<SimConnection>,
}

/// What both ends of a connection share.
struct Pair {
    closed: CancellationToken,
}

impl Fabric {
    /// A network whose random choices are decided by `seed`.
    pub(super) fn new(seed: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                listeners: HashMap::new(),
                partitions: HashSet::new(),
                pairs: Vec::new(),
                latency: Duration::from_millis(1),
                jitter: Duration::ZERO,
                rng: StdRng::seed_from_u64(seed),
                next_instance: 0,
            })),
        }
    }

    /// Every message takes `latency` plus up to `jitter` to arrive.
    pub(super) fn set_latency(&self, latency: Duration, jitter: Duration) {
        let mut state = self.state();
        state.latency = latency;
        state.jitter = jitter;
    }

    /// Cuts the links between every node in `a` and every node in `b`, and the
    /// connections across them.
    pub(super) fn partition(&self, a: &[&str], b: &[&str]) {
        let mut state = self.state();
        for &x in a {
            for &y in b {
                state
                    .partitions
                    .insert((NodeName::new(x), NodeName::new(y)));
                state
                    .partitions
                    .insert((NodeName::new(y), NodeName::new(x)));
            }
        }
        let State {
            pairs, partitions, ..
        } = &mut *state;
        pairs.retain(|(x, y, pair)| {
            let Some(pair) = pair.upgrade() else {
                return false;
            };
            if partitions.contains(&(x.clone(), y.clone())) {
                pair.closed.cancel();
                return false;
            }
            true
        });
    }

    /// Restores every link.
    pub(super) fn heal(&self) {
        self.state().partitions.clear();
    }

    /// Starts `node` listening on `addr`, replacing whatever listened there.
    pub(super) fn bind(&self, node: &str, addr: NodeAddr) -> SimEndpoint {
        let (incoming_tx, incoming_rx) = mpsc::channel(INCOMING_QUEUE);
        let mut state = self.state();
        let instance = state.next_instance;
        state.next_instance += 1;
        state.listeners.insert(
            addr.clone(),
            Listener {
                node: NodeName::new(node),
                instance,
                incoming: incoming_tx,
            },
        );
        SimEndpoint {
            fabric: self.clone(),
            node: NodeName::new(node),
            addr,
            instance,
            incoming: tokio::sync::Mutex::new(incoming_rx),
            connections: Mutex::new(Vec::new()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("Not poisoned")
    }

    fn partitioned(&self, from: &NodeName, to: &NodeName) -> bool {
        self.state()
            .partitions
            .contains(&(from.clone(), to.clone()))
    }

    /// How long a message takes to arrive, this time.
    fn delay(&self) -> Duration {
        let mut state = self.state();
        let jitter = state.jitter.mul_f64(state.rng.random_range(0.0..=1.0));
        state.latency + jitter
    }

    /// A one-way pipe of bytes from `from` to `to`, held back by the latency of
    /// the link. What is written in order comes out in order.
    fn pipe(
        &self,
        from: &NodeName,
        to: &NodeName,
        closed: CancellationToken,
    ) -> (DuplexStream, DuplexStream) {
        let (writer, mut ingress) = tokio::io::duplex(PIPE_BUFFER);
        let (mut egress, reader) = tokio::io::duplex(PIPE_BUFFER);
        // What has been read from the writer, and when it may come out.
        let (queue_tx, mut queue_rx) = mpsc::unbounded_channel::<(Instant, Option<Bytes>)>();

        let fabric = self.clone();
        let stop = closed.clone();
        tokio::spawn(async move {
            let mut last = Instant::now();
            loop {
                let mut buf = BytesMut::with_capacity(16 * 1024);
                let read = tokio::select! {
                    biased;
                    _ = stop.cancelled() => break,
                    read = ingress.read_buf(&mut buf) => read,
                };
                // Never out of order, whatever the jitter.
                last = last.max(Instant::now() + fabric.delay());
                match read {
                    Ok(n) if n > 0 => {
                        let _ = queue_tx.send((last, Some(buf.freeze())));
                    }
                    // The writer is done.
                    _ => {
                        let _ = queue_tx.send((last, None));
                        break;
                    }
                }
            }
        });

        let fabric = self.clone();
        let (from, to) = (from.clone(), to.clone());
        tokio::spawn(async move {
            loop {
                let item = tokio::select! {
                    biased;
                    _ = closed.cancelled() => break,
                    item = queue_rx.recv() => item,
                };
                let Some((deadline, chunk)) = item else { break };
                tokio::select! {
                    biased;
                    _ = closed.cancelled() => break,
                    _ = sleep_until(deadline) => {}
                }
                if fabric.partitioned(&from, &to) {
                    continue;
                }
                match chunk {
                    Some(bytes) => {
                        if egress.write_all(&bytes).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        let _ = egress.shutdown().await;
                        break;
                    }
                }
            }
        });
        (writer, reader)
    }
}

/// A node listening on the [`Fabric`]. Dropping it takes the node off the
/// network, like a crash.
pub struct SimEndpoint {
    fabric: Fabric,
    node: NodeName,
    addr: NodeAddr,
    instance: u64,
    incoming: tokio::sync::Mutex<mpsc::Receiver<SimConnection>>,
    /// Every connection this endpoint has made or accepted.
    connections: Mutex<Vec<Weak<Pair>>>,
}

impl SimEndpoint {
    fn track(&self, connection: &SimConnection) {
        self.connections
            .lock()
            .expect("Not poisoned")
            .push(Arc::downgrade(&connection.pair));
    }

    fn close_all(&self) {
        for pair in self.connections.lock().expect("Not poisoned").drain(..) {
            if let Some(pair) = pair.upgrade() {
                pair.closed.cancel();
            }
        }
    }
}

impl Endpoint for SimEndpoint {
    type Connection = SimConnection;

    fn local_addr(&self) -> io::Result<NodeAddr> {
        Ok(self.addr.clone())
    }

    async fn connect(&self, addr: &NodeAddr, node: &NodeName) -> io::Result<SimConnection> {
        let refused = || io::Error::from(io::ErrorKind::ConnectionRefused);
        // Like dialing an address: whoever answers there must be the node we
        // mean, and reachable from here.
        let incoming = {
            let state = self.fabric.state();
            state
                .listeners
                .get(addr)
                .filter(|listener| listener.node == *node)
                .filter(|_| {
                    !state
                        .partitions
                        .contains(&(self.node.clone(), node.clone()))
                })
                .map(|listener| listener.incoming.clone())
                .ok_or_else(refused)?
        };
        sleep_until(Instant::now() + self.fabric.delay()).await;

        let pair = Arc::new(Pair {
            closed: CancellationToken::new(),
        });
        self.fabric
            .state()
            .pairs
            .push((self.node.clone(), node.clone(), Arc::downgrade(&pair)));
        let (dialed, accepted) = SimConnection::pair(&self.fabric, pair, &self.node, node);
        incoming.send(accepted).await.map_err(|_| refused())?;
        self.track(&dialed);
        Ok(dialed)
    }

    async fn accept(&self) -> io::Result<SimConnection> {
        let connection = self
            .incoming
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| io::Error::from(io::ErrorKind::ConnectionAborted))?;
        self.track(&connection);
        Ok(connection)
    }

    async fn close(&self, _grace: Duration) {
        // Let what was just sent get through before the connections go.
        tokio::task::yield_now().await;
        self.close_all();
    }
}

impl Drop for SimEndpoint {
    fn drop(&mut self) {
        self.close_all();
        let mut state = self.fabric.state();
        if state
            .listeners
            .get(&self.addr)
            .is_some_and(|listener| listener.instance == self.instance)
        {
            state.listeners.remove(&self.addr);
        }
    }
}

type StreamPair = (SendStream, RecvStream);

/// One end of a connection between two simulated nodes.
pub struct SimConnection {
    fabric: Fabric,
    pair: Arc<Pair>,
    local: NodeName,
    remote: NodeName,
    /// Streams and datagrams to the other end.
    streams_out: mpsc::UnboundedSender<StreamPair>,
    datagrams_out: mpsc::UnboundedSender<Bytes>,
    /// Streams and datagrams from the other end.
    streams_in: tokio::sync::Mutex<mpsc::UnboundedReceiver<StreamPair>>,
    datagrams_in: tokio::sync::Mutex<mpsc::UnboundedReceiver<Bytes>>,
}

impl SimConnection {
    /// Both ends of a new connection, as the dialer and the acceptor see it.
    fn pair(
        fabric: &Fabric,
        pair: Arc<Pair>,
        dialer: &NodeName,
        acceptor: &NodeName,
    ) -> (Self, Self) {
        let (streams_a, streams_b) = (mpsc::unbounded_channel(), mpsc::unbounded_channel());
        let (datagrams_a, datagrams_b) = (mpsc::unbounded_channel(), mpsc::unbounded_channel());
        let end = |local: &NodeName,
                   remote: &NodeName,
                   streams: (
            mpsc::UnboundedSender<StreamPair>,
            mpsc::UnboundedReceiver<StreamPair>,
        ),
                   datagrams: (mpsc::UnboundedSender<Bytes>, mpsc::UnboundedReceiver<Bytes>)| {
            Self {
                fabric: fabric.clone(),
                pair: pair.clone(),
                local: local.clone(),
                remote: remote.clone(),
                streams_out: streams.0,
                datagrams_out: datagrams.0,
                streams_in: tokio::sync::Mutex::new(streams.1),
                datagrams_in: tokio::sync::Mutex::new(datagrams.1),
            }
        };
        // Each end sends into the other's receiver.
        let (streams_a_tx, streams_a_rx) = streams_a;
        let (streams_b_tx, streams_b_rx) = streams_b;
        let (datagrams_a_tx, datagrams_a_rx) = datagrams_a;
        let (datagrams_b_tx, datagrams_b_rx) = datagrams_b;
        (
            end(
                dialer,
                acceptor,
                (streams_b_tx, streams_a_rx),
                (datagrams_b_tx, datagrams_a_rx),
            ),
            end(
                acceptor,
                dialer,
                (streams_a_tx, streams_b_rx),
                (datagrams_a_tx, datagrams_b_rx),
            ),
        )
    }

    fn closed_error() -> io::Error {
        io::Error::from(io::ErrorKind::ConnectionReset)
    }
}

impl Connection for SimConnection {
    async fn open_stream(&self) -> io::Result<(SendStream, RecvStream)> {
        if self.pair.closed.is_cancelled() {
            return Err(Self::closed_error());
        }
        let closed = &self.pair.closed;
        let (out_writer, out_reader) = self.fabric.pipe(&self.local, &self.remote, closed.clone());
        let (back_writer, back_reader) =
            self.fabric.pipe(&self.remote, &self.local, closed.clone());
        self.streams_out
            .send((Box::new(back_writer), Box::new(out_reader)))
            .map_err(|_| Self::closed_error())?;
        Ok((Box::new(out_writer), Box::new(back_reader)))
    }

    async fn accept_stream(&self) -> io::Result<(SendStream, RecvStream)> {
        let mut streams = self.streams_in.lock().await;
        tokio::select! {
            biased;
            _ = self.pair.closed.cancelled() => Err(Self::closed_error()),
            stream = streams.recv() => stream.ok_or_else(Self::closed_error),
        }
    }

    fn send_datagram(&self, data: Bytes) -> Result<(), DatagramError> {
        if self.pair.closed.is_cancelled() {
            return Err(DatagramError::Closed);
        }
        if data.len() > MAX_DATAGRAM {
            return Err(DatagramError::TooLarge);
        }
        // Lost, when the link is cut.
        if self.fabric.partitioned(&self.local, &self.remote) {
            return Ok(());
        }
        let (out, delay) = (self.datagrams_out.clone(), self.fabric.delay());
        // Independent of each other: may overtake the ones before it.
        tokio::spawn(async move {
            sleep_until(Instant::now() + delay).await;
            let _ = out.send(data);
        });
        Ok(())
    }

    async fn recv_datagram(&self) -> io::Result<Bytes> {
        let mut datagrams = self.datagrams_in.lock().await;
        tokio::select! {
            biased;
            _ = self.pair.closed.cancelled() => Err(Self::closed_error()),
            datagram = datagrams.recv() => datagram.ok_or_else(Self::closed_error),
        }
    }

    /// The fabric checks that whoever answers is the node dialed, and knows
    /// who is dialing.
    fn peer(&self) -> &NodeName {
        &self.remote
    }

    fn is_closed(&self) -> bool {
        self.pair.closed.is_cancelled()
    }

    fn close(&self) {
        self.pair.closed.cancel();
    }
}

impl Drop for SimConnection {
    fn drop(&mut self) {
        self.pair.closed.cancel();
    }
}
