//! The monitors other nodes have asked this one for, see
//! [`MonitorOp`](super::ops::MonitorOp).
//!
//! A monitor outlives the message that asked for it, so unlike a call it has to be
//! cleaned up by hand. There are three ways one ends: the actor reaches a status
//! the monitoring node asked about, the monitoring node calls it off, or the monitoring node is
//! lost — the last being the one no message announces, see [`Monitors::drop_node`].

use crate::NodeName;
use dashmap::DashMap;
use tokio_util::sync::CancellationToken;

/// The monitors this node is holding for other nodes.
///
/// A `DashMap` rather than the immutable map the handlers use: monitors come and
/// go while the node runs, which is exactly what registration no longer does.
#[derive(Default)]
pub(crate) struct Monitors {
    /// Keyed by the monitoring node as well as the id, because the id is only
    /// unique within the node that minted it.
    live: DashMap<(NodeName, u64), CancellationToken>,
}

impl Monitors {
    /// Starts monitoring for `node`, and hands back the token that calls it off.
    pub(super) fn begin(&self, node: NodeName, monitor_id: u64) -> CancellationToken {
        let token = CancellationToken::new();
        self.live.insert((node, monitor_id), token.clone());
        token
    }

    /// The monitor is answered, or gave up. Forgetting an entry that isn't there
    /// is fine: `cancel` may have removed it a moment ago.
    pub(super) fn end(&self, node: &NodeName, monitor_id: u64) {
        self.live.remove(&(node.clone(), monitor_id));
    }

    /// Calls off one monitor. A miss is normal — the monitor may have just been
    /// answered — so this says nothing about it.
    pub(super) fn cancel(&self, node: &NodeName, monitor_id: u64) {
        if let Some((_, token)) = self.live.remove(&(node.clone(), monitor_id)) {
            token.cancel();
        }
    }

    /// Drops every monitor `node` asked for, because `node` is gone and will
    /// never call them off itself.
    ///
    /// This is the mirror of [`Pending::fail_node`](super::pending::Pending::fail_node),
    /// and the two are easy to confuse: that one ends the calls *this* node made
    /// *to* the peer, this one ends the monitors the *peer* asked *of* this node.
    pub(crate) fn drop_node(&self, node: &NodeName) {
        self.live.retain(|(monitoring_node, _), token| {
            let keep = monitoring_node != node;
            if !keep {
                token.cancel();
            }
            keep
        });
    }

    pub(crate) fn len(&self) -> usize {
        self.live.len()
    }
}
