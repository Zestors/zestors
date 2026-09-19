//! The task behind one sender: connects on demand, backs off while the peer is
//! unreachable, reports when that changes, and writes messages to the peer.

use super::{
    Conn, Delivery, Inner, PeerEvent, Protocol,
    wire::{MAX_MESSAGE_SIZE, write_frame},
};
use crate::{
    Addr, ClusterTimings, NodeId,
    backend::{DatagramError, RecvStream, SendStream},
};
use bytes::{BufMut, Bytes, BytesMut};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
    time::{Instant, timeout},
};

/// How many connection attempts in a row must fail before a peer is reported
/// [`PeerEvent::Unreachable`].
const UNREACHABLE_AFTER: u32 = 3;

/// How reaching one peer is going. Shared by all the lanes to that peer, so
/// that they agree on when it is unreachable and back off together.
#[derive(Default)]
pub(super) struct Health {
    /// Consecutive failed attempts to connect.
    failures: u32,
    /// When the next attempt may happen.
    retry_at: Option<Instant>,
    reported_unreachable: bool,
}

impl Health {
    fn backing_off(&self) -> bool {
        self.retry_at.is_some_and(|at| Instant::now() < at)
    }

    /// Connecting worked. Returns whether the peer had been reported unreachable.
    fn succeeded(&mut self) -> bool {
        self.failures = 0;
        self.retry_at = None;
        std::mem::take(&mut self.reported_unreachable)
    }

    /// Connecting failed. Returns how long to back off, and whether this is
    /// what makes the peer unreachable.
    fn failed(&mut self, timings: &ClusterTimings) -> (Duration, bool) {
        self.failures = self.failures.saturating_add(1);
        let backoff = backoff(timings, self.failures);
        self.retry_at = Some(Instant::now() + backoff);
        let newly_unreachable = self.failures >= UNREACHABLE_AFTER && !self.reported_unreachable;
        self.reported_unreachable |= newly_unreachable;
        (backoff, newly_unreachable)
    }
}

/// The long-lived stream of an ordered lane.
struct Ordered {
    send: SendStream,
    /// Read to the end to know the peer has received everything.
    recv: RecvStream,
    /// The connection it is on.
    conn: u64,
}

pub(super) async fn lane(
    inner: Arc<Inner>,
    node: NodeId,
    addr: Addr,
    protocol: Protocol,
    delivery: Delivery,
    health: Arc<Mutex<Health>>,
    mut rx: mpsc::Receiver<Bytes>,
) {
    let health_of = || health.lock().expect("Not poisoned");
    let mut stream: Option<Ordered> = None;

    loop {
        let payload = match timeout(inner.timings.peer_idle, rx.recv()).await {
            Ok(Some(payload)) => payload,
            Ok(None) => break,
            // Idle. Keep going for a peer reported unreachable, so that the
            // report is always followed by a matching `Reachable`; the lane
            // ends when the peer is forgotten.
            Err(_) if health_of().reported_unreachable => continue,
            Err(_) => break,
        };

        if payload.len() > MAX_MESSAGE_SIZE {
            tracing::warn!(%node, len = payload.len(), "Message over the size limit, dropping it");
            continue;
        }
        if health_of().backing_off() {
            tracing::trace!(%node, "Backing off, dropping message");
            continue;
        }

        let conn = match inner.connection(&node, &addr).await {
            Ok(conn) => {
                if health_of().succeeded() {
                    tracing::debug!(%node, "Peer is reachable again");
                    let _ = inner
                        .peer_events
                        .send(PeerEvent::Reachable(node.clone()))
                        .await;
                }
                conn
            }
            Err(err) => {
                let (backoff, newly_unreachable) = health_of().failed(&inner.timings);
                tracing::debug!(%node, %addr, ?backoff, "Failed to connect to peer: {err}");
                if newly_unreachable {
                    let _ = inner
                        .peer_events
                        .send(PeerEvent::Unreachable(node.clone()))
                        .await;
                }
                continue;
            }
        };

        let sent = match delivery {
            Delivery::Datagram => send_datagram(&conn, protocol, &payload).await,
            Delivery::Ordered => send_ordered(&conn, protocol, &payload, &mut stream).await,
        };
        if let Err(err) = sent {
            tracing::debug!(%node, "Failed to send to peer: {err}");
            stream = None;
            inner.unregister(&node, conn.id);
        }
    }

    // Give the final message a moment to be received: the peer ends its half
    // of the stream once it has read everything on ours.
    if let Some(mut stream) = stream {
        let _ = stream.send.shutdown().await;
        let _ = timeout(Duration::from_secs(1), async {
            let mut buf = [0u8; 1];
            while matches!(stream.recv.read(&mut buf).await, Ok(n) if n > 0) {}
        })
        .await;
    }
}

/// Sends `payload` as a datagram, or as a one-off stream if it can't be.
async fn send_datagram(conn: &Conn, protocol: Protocol, payload: &[u8]) -> std::io::Result<()> {
    let mut datagram = BytesMut::with_capacity(1 + payload.len());
    datagram.put_u8(protocol.0);
    datagram.put_slice(payload);

    match conn.send_datagram(datagram.freeze()) {
        Ok(()) => return Ok(()),
        // Doesn't fit in a packet, or there are no datagrams: a stream
        // delivers it just the same.
        Err(DatagramError::TooLarge | DatagramError::Unsupported) => {}
        Err(_) => return Err(std::io::ErrorKind::ConnectionAborted.into()),
    }

    let (mut send, _recv) = conn.open_stream().await?;
    send.write_all(&[protocol.0]).await?;
    write_frame(&mut send, payload).await?;
    send.shutdown().await
}

/// Appends `payload` to the ordered stream, opening one first if there is none
/// on `conn`.
async fn send_ordered(
    conn: &Conn,
    protocol: Protocol,
    payload: &[u8],
    stream: &mut Option<Ordered>,
) -> std::io::Result<()> {
    // A stream on an earlier connection is gone with it.
    if stream.as_ref().is_none_or(|open| open.conn != conn.id) {
        *stream = None;
        let (mut send, recv) = conn.open_stream().await?;
        send.write_all(&[protocol.0]).await?;
        *stream = Some(Ordered {
            send,
            recv,
            conn: conn.id,
        });
    }

    let open = stream.as_mut().expect("Stream was just opened");
    write_frame(&mut open.send, payload).await
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
