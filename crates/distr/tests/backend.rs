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
    ClusterConfig, ClusterEvent, ClusterNode, NodeAddr, NodeName, Seed,
    backend::{
        Backend, Connection, DatagramError, Endpoint, NodeIncarnation, RecvStream, SendStream,
    },
};

type Listener = (NodeName, mpsc::Sender<HubConnection>);

/// Connects nodes of one process to each other through channels.
#[derive(Clone, Default)]
struct Hub {
    listeners: Arc<Mutex<HashMap<NodeAddr, Listener>>>,
}

struct HubBackend {
    hub: Hub,
    addr: NodeAddr,
    /// The name the hub knows this node by, if not the one it was started with.
    hub_name: Option<NodeName>,
}

impl Backend for HubBackend {
    type Endpoint = HubEndpoint;

    async fn start(self, local: NodeIncarnation) -> io::Result<HubEndpoint> {
        let node = self.hub_name.unwrap_or(local.name);
        let (tx, rx) = mpsc::channel(16);
        self.hub
            .listeners
            .lock()
            .unwrap()
            .insert(self.addr.clone(), (node.clone(), tx));
        Ok(HubEndpoint {
            hub: self.hub,
            addr: self.addr,
            node,
            incoming: tokio::sync::Mutex::new(rx),
        })
    }
}

struct HubEndpoint {
    hub: Hub,
    addr: NodeAddr,
    node: NodeName,
    incoming: tokio::sync::Mutex<mpsc::Receiver<HubConnection>>,
}

impl Endpoint for HubEndpoint {
    type Connection = HubConnection;

    fn local_addr(&self) -> io::Result<NodeAddr> {
        Ok(self.addr.clone())
    }

    async fn connect(&self, addr: &NodeAddr, node: &NodeName) -> io::Result<HubConnection> {
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
    peer: NodeName,
    streams_out: mpsc::UnboundedSender<Streams>,
    streams_in: tokio::sync::Mutex<mpsc::UnboundedReceiver<Streams>>,
    closed: Arc<AtomicBool>,
}

impl HubConnection {
    fn pair(dialer: &NodeName, acceptor: &NodeName) -> (Self, Self) {
        let (to_acceptor, from_dialer) = mpsc::unbounded_channel();
        let (to_dialer, from_acceptor) = mpsc::unbounded_channel();
        let closed = Arc::new(AtomicBool::new(false));
        let end = |peer: &NodeName, out, from| Self {
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

    fn peer(&self) -> &NodeName {
        &self.peer
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}

fn addr(n: u8) -> NodeAddr {
    NodeAddr::new(format!("hub-{n}"))
}

fn fast_foca() -> foca::Config {
    let mut foca = foca::Config::new_lan(NonZeroU32::new(3).unwrap());
    foca.probe_period = Duration::from_millis(100);
    foca.probe_rtt = Duration::from_millis(50);
    foca.suspect_to_down_after = Duration::from_millis(200);
    foca
}

fn config(hub: &Hub, name: &str, n: u8) -> ClusterConfig {
    let foca = fast_foca();
    ClusterConfig::new(
        name,
        HubBackend {
            hub: hub.clone(),
            addr: addr(n),
            hub_name: None,
        },
    )
    .foca_config(foca)
}

/// Shuts a node down through its root supervisor, once that takes signals: it
/// is initializing or running.
async fn stop(root: &zestors::runtime::Address<zestors_supervisor::SupervisorInterface>) {
    use zestors::runtime::prelude::*;
    root.watch_accepts_messages().await;
    root.signal_shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn nodes_join_over_a_custom_backend() {
    let hub = Hub::default();
    let a = ClusterNode::new(
        Supervisor::blueprint().rand_name(),
        config(&hub, "node-a", 1),
    );
    let b = ClusterNode::new(
        Supervisor::blueprint().rand_name(),
        config(&hub, "node-b", 2).seed(Seed::new("node-a", addr(1))),
    );

    let (a_cluster, b_cluster) = (a.cluster(), b.cluster());
    let b_shutdown = b.root_supervisor().address().clone();
    let a_shutdown = a.root_supervisor().address().clone();
    let a_task = tokio::spawn(a.run());
    let b_task = tokio::spawn(b.run());

    let wait = |cluster: zestors_distr::Cluster| async move {
        tokio::time::timeout(Duration::from_secs(30), cluster.wait_for_members(1))
            .await
            .expect("Timed out")
    };
    let mut a_events = a_cluster.subscribe();
    let members = wait(a_cluster.clone()).await;
    assert_eq!(members[0].name.as_str(), "node-b");
    assert_eq!(members[0].addr, addr(2));
    wait(b_cluster).await;

    // The departure of b arrives as a frame like any other.
    stop(&b_shutdown).await;
    b_task.await.unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(ClusterEvent::Left(member)) = a_events.recv().await
                && member.name.as_str() == "node-b"
            {
                break;
            }
        }
    })
    .await
    .expect("Timed out waiting for b to leave");

    stop(&a_shutdown).await;
    a_task.await.unwrap().unwrap();
}

/// The backend says who a node is. One that calls itself by another name than
/// the backend knows it by doesn't get in under that name.
#[tokio::test(flavor = "multi_thread")]
async fn a_node_cannot_get_in_under_a_name_the_backend_does_not_know_it_by() {
    let hub = Hub::default();
    let a = ClusterNode::new(
        Supervisor::blueprint().rand_name(),
        config(&hub, "node-a", 1),
    );
    let a_cluster = a.cluster();
    let a_shutdown = a.root_supervisor().address().clone();
    let a_task = tokio::spawn(a.run());

    // Known to the hub, and so to node-a, as "mallory", but gossiping as node-b.
    let mallory = ClusterConfig::new(
        "node-b",
        HubBackend {
            hub: hub.clone(),
            addr: addr(2),
            hub_name: Some(NodeName::new("mallory")),
        },
    )
    .seed(Seed::new("node-a", addr(1)))
    .foca_config(fast_foca());
    let mallory = ClusterNode::new(Supervisor::blueprint().rand_name(), mallory);
    let mallory_shutdown = mallory.root_supervisor().address().clone();
    let mallory_task = tokio::spawn(mallory.run());

    // An honest node joins meanwhile, so we know node-a is listening.
    let c = ClusterNode::new(
        Supervisor::blueprint().rand_name(),
        config(&hub, "node-c", 3).seed(Seed::new("node-a", addr(1))),
    );
    let c_shutdown = c.root_supervisor().address().clone();
    let c_task = tokio::spawn(c.run());
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if a_cluster.member(&NodeName::new("node-c")).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("The honest node joins");

    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(a_cluster.member(&NodeName::new("node-b")).is_none());
    assert_eq!(a_cluster.members().len(), 1);

    for root in [a_shutdown, mallory_shutdown, c_shutdown] {
        stop(&root).await;
    }
    for task in [a_task, mallory_task, c_task] {
        task.await.unwrap().unwrap();
    }
}
