//! The task that sends to one peer: connects on demand, backs off while the
//! peer is unreachable, and reports when that changes.

use super::{Inner, Outgoing, UNREACHABLE_AFTER, send_message};
use crate::{ClusterTimings, NodeId, cluster::net::Event};
use quinn::SendStream;
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::mpsc,
    time::{Instant, timeout},
};

pub(super) async fn peer_task(inner: Arc<Inner>, node: NodeId, mut rx: mpsc::Receiver<Outgoing>) {
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
