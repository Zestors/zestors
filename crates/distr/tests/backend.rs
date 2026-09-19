//! A cluster over a backend implemented outside the crate, using only the
//! public API: enough to plug in another way of carrying messages.

use bytes::Bytes;
use std::{
    collections::HashMap,
    io,
    num::NonZeroU32,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::mpsc;
use zestors::{prelude::*, supervisor::Supervisor};
use zestors_distr::{
    Addr, ClusterConfig, ClusterEvent, ClusterNode, NodeId, Seed,
    backend::{Backend, Connection, DatagramError, Endpoint, LocalNode, RecvStream, SendStream},
};

type Listener = (NodeId, mpsc::Sender<HubConnection>);

/// Connects nodes of one process to each other through channels.
#[derive(Clone, Default)]
struct Hub {
    listeners: Arc<Mutex<HashMap<Addr, Listener>>>,
}

struct HubBackend {
    hub: Hub,
    addr: Addr,
}

impl Backend for HubBackend {
    type Endpoint = HubEndpoint;

    async fn start(self, local: LocalNode) -> io::Result<HubEndpoint> {
        let (tx, rx) = mpsc::channel(16);
        self.hub
            .listeners
            .lock()
            .unwrap()
            .insert(self.addr.clone(), (local.id.clone(), tx));
        Ok(HubEndpoint {
            hub: self.hub,
            addr: self.addr,
            node: local.id,
            incoming: tokio::sync::Mutex::new(rx),
        })
    }
}

struct HubEndpoint {
    hub: Hub,
    addr: Addr,
    node: NodeId,
    incoming: tokio::sync::Mutex<mpsc::Receiver<HubConnection>>,
}

impl Endpoint for HubEndpoint {
    type Connection = HubConnection;

    fn local_addr(&self) -> io::Result<Addr> {
        Ok(self.addr.clone())
    }

    async fn connect(&self, addr: &Addr, node: &NodeId) -> io::Result<HubConnection> {
        let incoming = match self.hub.listeners.lock().unwrap().get(addr) {
            Some((name, tx)) if name == node => tx.clone(),
            _ => return Err(io::ErrorKind::ConnectionRefused.into()),
        };
        let (dialed, accepted) = HubConnection::pair(&self.node, node);
        incoming
            .send(accepted)
            .await
            .map_err(|_| io::Error::from(io::ErrorKind::ConnectionRefused))?;
        Ok(dialed)
    }

    async fn accept(&self) -> io::Result<HubConnection> {
        (self.incoming.lock().await.recv().await)
            .ok_or_else(|| io::ErrorKind::ConnectionAborted.into())
    }

    async fn close(&self, _grace: Duration) {
        self.hub.listeners.lock().unwrap().remove(&self.addr);
    }
}

type Streams = (SendStream, RecvStream);

/// One end of a connection. Streams are in-memory pipes; there are no datagrams,
/// so the cluster has to make do with streams.
struct HubConnection {
    peer: NodeId,
    streams_out: mpsc::UnboundedSender<Streams>,
    streams_in: tokio::sync::Mutex<mpsc::UnboundedReceiver<Streams>>,
    closed: Arc<AtomicBool>,
}

impl HubConnection {
    fn pair(dialer: &NodeId, acceptor: &NodeId) -> (Self, Self) {
        let (to_acceptor, from_dialer) = mpsc::unbounded_channel();
        let (to_dialer, from_acceptor) = mpsc::unbounded_channel();
        let closed = Arc::new(AtomicBool::new(false));
        let end = |peer: &NodeId, out, from| Self {
            peer: peer.clone(),
            streams_out: out,
            streams_in: tokio::sync::Mutex::new(from),
            closed: closed.clone(),
        };
        (
            end(acceptor, to_acceptor, from_acceptor),
            end(dialer, to_dialer, from_dialer),
        )
    }
}

impl Drop for HubConnection {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}

impl Connection for HubConnection {
    async fn open_stream(&self) -> io::Result<Streams> {
        let (mine, theirs) = tokio::io::duplex(64 * 1024);
        let (their_send, my_recv) = tokio::io::duplex(64 * 1024);
        self.streams_out
            .send((Box::new(their_send), Box::new(theirs)))
            .map_err(|_| io::Error::from(io::ErrorKind::ConnectionReset))?;
        Ok((Box::new(mine), Box::new(my_recv)))
    }

    async fn accept_stream(&self) -> io::Result<Streams> {
        (self.streams_in.lock().await.recv().await)
            .ok_or_else(|| io::ErrorKind::ConnectionReset.into())
    }

    fn send_datagram(&self, _data: Bytes) -> Result<(), DatagramError> {
        Err(DatagramError::Unsupported)
    }

    async fn recv_datagram(&self) -> io::Result<Bytes> {
        std::future::pending().await
    }

    fn authenticates(&self, node: &NodeId) -> bool {
        *node == self.peer
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}

fn addr(n: u8) -> Addr {
    Addr::new(format!("hub-{n}"))
}

fn config(hub: &Hub, name: &str, n: u8) -> ClusterConfig {
    let mut foca = foca::Config::new_lan(NonZeroU32::new(3).unwrap());
    foca.probe_period = Duration::from_millis(100);
    foca.probe_rtt = Duration::from_millis(50);
    foca.suspect_to_down_after = Duration::from_millis(200);
    ClusterConfig::with_backend(
        name,
        HubBackend {
            hub: hub.clone(),
            addr: addr(n),
        },
    )
    .foca_config(foca)
}

#[tokio::test(flavor = "multi_thread")]
async fn nodes_join_over_a_custom_backend() {
    let hub = Hub::default();
    let a = ClusterNode::new(
        Supervisor::blueprint().rand_pid(),
        config(&hub, "node-a", 1),
    )
    .with_exit_delay(Duration::ZERO);
    let b = ClusterNode::new(
        Supervisor::blueprint().rand_pid(),
        config(&hub, "node-b", 2).seed(Seed::new("node-a", addr(1))),
    )
    .with_exit_delay(Duration::ZERO);

    let (a_cluster, b_cluster) = (a.cluster(), b.cluster());
    let b_shutdown = b.shutdown_handle();
    let a_shutdown = a.shutdown_handle();
    let a_task = tokio::spawn(a.run());
    let b_task = tokio::spawn(b.run());

    let wait = |cluster: zestors_distr::Cluster| async move {
        tokio::time::timeout(Duration::from_secs(30), cluster.wait_for_members(1))
            .await
            .expect("Timed out")
    };
    let mut a_events = a_cluster.subscribe();
    let members = wait(a_cluster.clone()).await;
    assert_eq!(members[0].node.as_str(), "node-b");
    assert_eq!(members[0].addr, addr(2));
    wait(b_cluster).await;

    // The departure of b arrives as a frame like any other.
    b_shutdown.shutdown();
    b_task.await.unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(ClusterEvent::Left(member)) = a_events.recv().await
                && member.node.as_str() == "node-b"
            {
                break;
            }
        }
    })
    .await
    .expect("Timed out waiting for b to leave");

    a_shutdown.shutdown();
    a_task.await.unwrap().unwrap();
}
