use zestors_runtime::{
    Registry, {ActorStatus, ChannelSnapshot},
};

use crate::_prelude::*;
use std::collections::VecDeque;

/// A recursive snapshot of a supervisor and its descendants: for each node,
/// its [`ChildDescription`], live [`ActorStatus`] (if still registered), and
/// optionally its [`Health`] and [`ChannelSnapshot`].
///
/// Built with [`SupervisionTree::new`] (a single, unpopulated node) and then
/// [`SupervisionTree::populated`]/[`SupervisionTree::populate`] (which walks
/// down through [`GetChildren`](crate::messages::GetChildren) to discover and
/// fill in every descendant).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SupervisionTree {
    /// This node's identity and configuration.
    pub description: ChildDescription,
    /// This node's live status, or `None` if it's no longer registered.
    pub status: Option<ActorStatus>,

    /// This node's health, if [`SupervisionTree::populate`] hasn't been
    /// extended to fetch it — currently always `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<Health>,

    /// This node's channel snapshot, if [`SupervisionTree::populate_channel_snapshots`]
    /// has been called.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_state: Option<ChannelSnapshot>,

    /// This node's direct children, recursively populated the same way.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SupervisionTree>,
}

impl SupervisionTree {
    /// Creates a single, unpopulated node for `description`, with its
    /// current [`ActorStatus`] (if it's still registered) but no children.
    pub fn new(description: ChildDescription) -> Self {
        Self {
            status: Registry::local()
                .get(&description.name)
                .map(|address| address.status()),
            description,
            health: None,
            channel_state: None,
            children: Vec::new(),
        }
    }

    /// Builder-style version of [`SupervisionTree::populate`].
    pub async fn populated(mut self, timeout: Duration) -> Self {
        self.populate(timeout).await;
        self
    }

    /// Recursively discovers and fills in every descendant, by querying
    /// each node (breadth-first) for its children, waiting up to `timeout`
    /// for each query. A node that fails to respond in time or errors is
    /// simply left with no children, rather than failing the whole walk.
    pub async fn populate(&mut self, timeout: Duration) -> () {
        let mut queue = VecDeque::new();
        queue.push_back(self);

        while let Some(node) = queue.pop_front() {
            node.populate_layer(timeout).await;

            for child in &mut node.children {
                queue.push_back(child);
            }
        }
    }

    async fn populate_layer(&mut self, timeout: Duration) {
        let Some(address) = Registry::local().get(&self.description.name) else {
            return;
        };

        let children = match tokio::time::timeout(timeout, address.call_dyn(GetChildren)).await {
            Ok(Ok(children)) => children,
            Ok(Err(err)) => {
                tracing::warn!(
                    "Failed to get children for name {}: {}",
                    self.description.name,
                    err
                );
                vec![]
            }
            Err(_) => {
                tracing::warn!(
                    "Timeout getting children for name {}",
                    self.description.name
                );
                vec![]
            }
        };

        for child in children {
            self.children.push(SupervisionTree::new(child));
        }
    }

    /// Fills in [`SupervisionTree::channel_state`] for this node and every
    /// already-discovered descendant, from the local [`Registry`] (so, unlike
    /// [`SupervisionTree::populate`], this never needs to wait on the
    /// supervisees themselves and can't time out).
    pub fn populate_channel_snapshots(&mut self) {
        let mut queue = VecDeque::new();
        queue.push_back(self);

        while let Some(node) = queue.pop_front() {
            let address = match Registry::local().get(&node.description.name) {
                Some(address) => address,
                None => {
                    continue;
                }
            };

            let channel_state = address.snapshot();
            node.channel_state = Some(channel_state);

            for child in &mut node.children {
                queue.push_back(child);
            }
        }
    }

    /// Builder-style version of [`SupervisionTree::populate_channel_snapshots`].
    pub fn populated_channel_snapshots(mut self) -> Self {
        self.populate_channel_snapshots();
        self
    }
}
