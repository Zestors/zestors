use std::collections::HashMap;
use std::task::Poll;

use futures::FutureExt;
use indexmap::IndexMap;

use crate::core::*;
use crate::distr::*;

/// All NodeChildren that an EndpointActor owns.
#[derive(Debug)]
pub(crate) struct NodeChildren(IndexMap<NodeId, NodeChild>);

impl NodeChildren {
    pub(crate) fn new() -> Self {
        Self(IndexMap::new())
    }

    /// Attempt to store a new node.
    /// Fails if the node was already stored
    pub(crate) fn store_node(&mut self, system_node: NodeChild) -> Result<(), NodeChild> {
        let id = system_node.id().clone();
        match self.0.contains_key(&id) {
            true => Err(system_node),
            false => {
                self.0.insert(id, system_node);
                Ok(())
            }
        }
    }

    /// Attempt to remove a node.
    pub(crate) fn remove_node(&mut self, id: &NodeId) -> Option<NodeChild> {
        self.0.remove(id)
    }

    pub(crate) fn get_node(&self, id: &NodeId) -> Option<&NodeChild> {
        self.0.get(id)
    }

    pub(crate) fn get_node_mut(&mut self, id: &NodeId) -> Option<&mut NodeChild> {
        self.0.get_mut(id)
    }
}

/// Stream over the node-children to await their exits.
impl Stream for NodeChildren {
    type Item = (
        NodeId,
        Result<<NodeActor as Actor>::Exit, ExitError>,
        NodeChild,
    );

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let ready = self
            .0
            .iter_mut()
            .find_map(|(id, child)| match child.poll_unpin(cx) {
                Poll::Ready(exit) => Some((id.clone(), exit)),
                Poll::Pending => None,
            });

        match ready {
            Some((id, exit)) => {
                let child = self.0.remove(&id).unwrap();
                Poll::Ready(Some((id, exit, child)))
            }
            None => Poll::Pending,
        }
    }
}
