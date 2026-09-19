//! QUIC as a [`Backend`]: mutually authenticated, with native streams and
//! datagrams. This only adapts `quinn`; connection management and message
//! handling are done by the cluster.

mod tls;

pub use tls::{Tls, TlsError};

use super::{Backend, Connection, DatagramError, Endpoint, LocalNode, RecvStream, SendStream};
use crate::{Addr, NodeId};
use bytes::Bytes;
use std::{io, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{sync::mpsc, time::timeout};
use tokio_util::sync::{CancellationToken, DropGuard};

/// The timeouts of the [`Quic`] backend. The defaults suit real networks;
/// shorten them for local tests.
#[derive(Debug, Clone)]
pub struct QuicTimings {
    /// How often idle connections send a keep-alive packet. Must be shorter
    /// than `idle_timeout`.
    pub keep_alive: Duration,
    /// How long a connection may go without any packets before it is dropped.
    pub idle_timeout: Duration,
}

impl Default for QuicTimings {
    fn default() -> Self {
        Self {
            keep_alive: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(30),
        }
    }
}

/// The default [`Backend`]: mutually authenticated QUIC.
///
/// Every node presents a certificate that the others verify (see [`Tls`]), and
/// is dialed by its node name, which its certificate must carry as a DNS name.
pub struct Quic {
    bind: SocketAddr,
    tls: Tls,
    timings: QuicTimings,
}

impl Quic {
    /// A QUIC backend that listens on `bind`, with the identity `tls`.
    pub fn new(bind: SocketAddr, tls: Tls) -> Self {
        Self {
            bind,
            tls,
            timings: QuicTimings::default(),
        }
    }

    /// Sets the timeouts.
    pub fn timings(mut self, timings: QuicTimings) -> Self {
        self.timings = timings;
        self
    }
}

impl Backend for Quic {
    type Endpoint = QuicEndpoint;

    async fn start(self, local: LocalNode) -> io::Result<QuicEndpoint> {
        // The node name doubles as the TLS server name when peers dial it.
        if quinn::rustls::pki_types::ServerName::try_from(local.id.as_str()).is_err() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Invalid node name {:?}: must be a valid DNS name", local.id),
            ));
        }

        let mut transport = quinn::TransportConfig::default();
        transport.keep_alive_interval(Some(self.timings.keep_alive));
        transport.max_idle_timeout(Some(
            quinn::IdleTimeout::try_from(self.timings.idle_timeout)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
        ));
        let (server, client) = self.tls.quic_configs(Arc::new(transport))?;
        let mut endpoint = quinn::Endpoint::server(server, self.bind)?;
        endpoint.set_default_client_config(client);

        // Finishing the handshake of one peer must not hold up the others.
        let (ready_tx, ready_rx) = mpsc::channel(64);
        let token = CancellationToken::new();
        let accepting = endpoint.clone();
        let accepting_tls = self.tls.clone();
        tokio::spawn(token.clone().run_until_cancelled_owned(async move {
            while let Some(incoming) = accepting.accept().await {
                let (ready, tls) = (ready_tx.clone(), accepting_tls.clone());
                tokio::spawn(async move {
                    match incoming.await {
                        Ok(conn) => {
                            let _ = ready.send(QuicConnection { conn, tls }).await;
                        }
                        Err(err) => tracing::debug!("Incoming connection failed: {err}"),
                    }
                });
            }
        }));

        Ok(QuicEndpoint {
            endpoint,
            tls: self.tls,
            ready: tokio::sync::Mutex::new(ready_rx),
            _stop_accepting: token.drop_guard(),
        })
    }
}

/// The [`Endpoint`] of [`Quic`].
pub struct QuicEndpoint {
    endpoint: quinn::Endpoint,
    tls: Tls,
    ready: tokio::sync::Mutex<mpsc::Receiver<QuicConnection>>,
    _stop_accepting: DropGuard,
}

impl Endpoint for QuicEndpoint {
    type Connection = QuicConnection;

    fn local_addr(&self) -> io::Result<Addr> {
        self.endpoint.local_addr().map(Addr::from)
    }

    async fn connect(&self, addr: &Addr, node: &NodeId) -> io::Result<QuicConnection> {
        let socket = resolve(addr).await?;
        let conn = self
            .endpoint
            .connect(socket, node.as_str())
            .map_err(io::Error::other)?
            .await?;
        Ok(QuicConnection {
            conn,
            tls: self.tls.clone(),
        })
    }

    async fn accept(&self) -> io::Result<QuicConnection> {
        self.ready
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| io::ErrorKind::ConnectionAborted.into())
    }

    async fn close(&self, grace: Duration) {
        self.endpoint.close(0u32.into(), b"shutdown");
        let _ = timeout(grace, self.endpoint.wait_idle()).await;
    }
}

/// The [`Connection`] of [`Quic`].
pub struct QuicConnection {
    conn: quinn::Connection,
    tls: Tls,
}

impl Connection for QuicConnection {
    async fn open_stream(&self) -> io::Result<(SendStream, RecvStream)> {
        let (send, recv) = self.conn.open_bi().await?;
        Ok((Box::new(send), Box::new(recv)))
    }

    async fn accept_stream(&self) -> io::Result<(SendStream, RecvStream)> {
        let (send, recv) = self.conn.accept_bi().await?;
        Ok((Box::new(send), Box::new(recv)))
    }

    fn send_datagram(&self, data: Bytes) -> Result<(), DatagramError> {
        self.conn.send_datagram(data).map_err(|err| match err {
            quinn::SendDatagramError::TooLarge => DatagramError::TooLarge,
            quinn::SendDatagramError::UnsupportedByPeer | quinn::SendDatagramError::Disabled => {
                DatagramError::Unsupported
            }
            quinn::SendDatagramError::ConnectionLost(_) => DatagramError::Closed,
        })
    }

    async fn recv_datagram(&self) -> io::Result<Bytes> {
        Ok(self.conn.read_datagram().await?)
    }

    fn authenticates(&self, node: &NodeId) -> bool {
        match self.tls.verify_peer(&self.conn, node) {
            Ok(()) => true,
            Err(err) => {
                tracing::debug!(%node, "Peer rejected: {err}");
                false
            }
        }
    }

    fn is_closed(&self) -> bool {
        self.conn.close_reason().is_some()
    }

    fn close(&self) {
        self.conn.close(0u32.into(), b"closed");
    }
}

/// The socket address `addr` stands for: itself if it is one, else the first
/// address its host name resolves to.
async fn resolve(addr: &Addr) -> io::Result<SocketAddr> {
    if let Some(socket) = addr.to_socket_addr() {
        return Ok(socket);
    }
    tokio::net::lookup_host(addr.as_str())
        .await?
        .next()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{addr} does not resolve to an address"),
            )
        })
}
