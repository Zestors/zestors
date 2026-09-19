use super::{ClusterEvent, ClusterSnapshot, Member, NodeStatus};
use crate::NodeName;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};
use tokio::sync::{broadcast, watch};

/// What this node knows about the cluster. Kept under one lock so that readers
/// never see the parts disagree, and so that events can be published with a
/// change made under it (see the mutators below).
struct State {
    local: Member,
    members: HashMap<NodeName, Member>,
    /// Members in `members` that can't currently be connected to.
    unreachable: HashSet<NodeName>,
}

struct Shared {
    status: watch::Sender<NodeStatus>,
    state: RwLock<State>,
    events: broadcast::Sender<ClusterEvent>,
}

/// A cheaply cloneable handle to a running cluster, obtained from
/// [`ClusterNode::cluster`](crate::ClusterNode::cluster).
///
/// It is usable before the node has started, in which case it reports no
/// members.
#[derive(Clone)]
pub struct Cluster {
    shared: Arc<Shared>,
}

impl Cluster {
    pub(super) fn new(local: Member) -> Self {
        Self {
            shared: Arc::new(Shared {
                status: watch::channel(NodeStatus::Starting).0,
                state: RwLock::new(State {
                    local,
                    members: HashMap::new(),
                    unreachable: HashSet::new(),
                }),
                events: broadcast::channel(256).0,
            }),
        }
    }

    /// This node, as the rest of the cluster knows it.
    pub fn local(&self) -> Member {
        self.state().local.clone()
    }

    fn state(&self) -> std::sync::RwLockReadGuard<'_, State> {
        self.shared.state.read().expect("Not poisoned")
    }

    pub(super) fn set_local(&self, local: Member) {
        self.shared.state.write().expect("Not poisoned").local = local;
    }

    /// The other nodes currently considered up, in no particular order.
    pub fn members(&self) -> Vec<Member> {
        self.state().members.values().cloned().collect()
    }

    /// The other node named `node`, if it is currently up.
    pub fn member(&self, node: &NodeName) -> Option<Member> {
        self.state().members.get(node).cloned()
    }

    /// Whether `node` is a member that this node can currently connect to.
    /// False for nodes that aren't members, and for those reported as
    /// [`ClusterEvent::Unreachable`].
    pub fn is_reachable(&self, node: &NodeName) -> bool {
        let state = self.state();
        state.members.contains_key(node) && !state.unreachable.contains(node)
    }

    /// Subscribes to membership changes.
    ///
    /// Receivers that fall too far behind miss events; resynchronize with
    /// [`Cluster::subscribe_with_snapshot`] or [`Cluster::members`]. To also
    /// know who the members are right now, use `subscribe_with_snapshot`: calling
    /// `members()` and `subscribe()` separately can miss or double count a change
    /// in between.
    pub fn subscribe(&self) -> broadcast::Receiver<ClusterEvent> {
        self.shared.events.subscribe()
    }

    /// The current members together with a subscription to every change after
    /// them: each change is either in the snapshot or delivered as an event,
    /// never both and never neither.
    pub fn subscribe_with_snapshot(&self) -> (ClusterSnapshot, broadcast::Receiver<ClusterEvent>) {
        // Changes are published under the state lock (see the mutators below).
        let state = self.state();
        let events = self.shared.events.subscribe();
        let snapshot = ClusterSnapshot {
            members: state.members.values().cloned().collect(),
            unreachable: state.unreachable.clone(),
        };
        (snapshot, events)
    }

    /// Waits until `condition` holds for the current members, and returns them.
    /// Returns immediately if it already does.
    ///
    /// The condition is checked against every change, so it may run often;
    /// keep it cheap. Never resolves if the condition never holds; wrap it in a
    /// timeout when that matters.
    pub async fn wait_until(&self, mut condition: impl FnMut(&[Member]) -> bool) -> Vec<Member> {
        let (snapshot, mut events) = self.subscribe_with_snapshot();
        let mut members = snapshot.members;
        loop {
            if condition(&members) {
                return members;
            }
            match events.recv().await {
                Ok(_) => {}
                // Missed some changes: start over from the current state.
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => {
                    unreachable!("The cluster holds the sender")
                }
            }
            members = self.members();
        }
    }

    /// Waits until exactly `count` other nodes are considered up.
    pub async fn wait_for_members(&self, count: usize) -> Vec<Member> {
        self.wait_until(|members| members.len() == count).await
    }

    /// What this node is currently doing.
    pub fn status(&self) -> NodeStatus {
        *self.shared.status.borrow()
    }

    /// Waits until [`Cluster::status`] is `status`.
    pub async fn wait_for_status(&self, status: NodeStatus) {
        let mut rx = self.shared.status.subscribe();
        let _ = rx.wait_for(|current| *current == status).await;
    }

    pub(super) fn set_status(&self, status: NodeStatus) {
        self.shared.status.send_if_modified(|current| {
            // Nothing follows Defunct or Leaving except leaving the cluster.
            let allowed = match (*current, status) {
                (NodeStatus::Defunct, _) => false,
                (NodeStatus::Leaving, next) => next != NodeStatus::Up,
                _ => true,
            };
            let changed = allowed && *current != status;
            if changed {
                *current = status;
            }
            changed
        });
    }
}

impl std::fmt::Debug for Cluster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cluster")
            .field("local", &self.local())
            .field("members", &self.members())
            .finish()
    }
}

// Every change to the cluster's membership goes through the mutators below,
// and is published while holding the state lock, so that
// [`Cluster::subscribe_with_snapshot`], which subscribes under the same lock,
// sees each change either in its snapshot or as an event, never both or neither.
impl Cluster {
    pub(super) fn contains(&self, member: &Member) -> bool {
        self.shared
            .state
            .read()
            .expect("Not poisoned")
            .members
            .get(&member.node)
            == Some(member)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, State> {
        self.shared.state.write().expect("Not poisoned")
    }

    fn publish(&self, event: ClusterEvent) {
        let _ = self.shared.events.send(event);
    }

    pub(super) fn member_up(&self, member: Member) {
        let mut state = self.write();
        let previous = state.members.insert(member.node.clone(), member.clone());
        if previous.as_ref() != Some(&member) {
            state.unreachable.remove(&member.node);
            self.publish(ClusterEvent::Up(member));
        }
    }

    /// This node can't connect to `node`. Returns the member if that is news.
    pub(super) fn member_unreachable(&self, node: &NodeName) -> Option<Member> {
        let mut state = self.write();
        let member = state.members.get(node)?.clone();
        state.unreachable.insert(node.clone()).then(|| {
            self.publish(ClusterEvent::Unreachable(member.clone()));
            member
        })
    }

    /// This node can connect to `node` again. Returns the member if it had been
    /// reported unreachable.
    pub(super) fn member_reachable(&self, node: &NodeName) -> Option<Member> {
        let mut state = self.write();
        let member = state.members.get(node)?.clone();
        state.unreachable.remove(node).then(|| {
            self.publish(ClusterEvent::Reachable(member.clone()));
            member
        })
    }

    /// The node said goodbye. Returns it if it was known in that generation.
    pub(super) fn member_left(&self, node: &NodeName, generation: u64) -> Option<Member> {
        let mut state = self.write();
        if state.members.get(node)?.generation != generation {
            return None;
        }
        let member = state.members.remove(node)?;
        state.unreachable.remove(node);
        self.publish(ClusterEvent::Left(member.clone()));
        Some(member)
    }

    /// The node was declared down without saying goodbye.
    pub(super) fn member_failed(&self, member: &Member) {
        let mut state = self.write();
        if state.members.get(&member.node) == Some(member) {
            state.members.remove(&member.node);
            state.unreachable.remove(&member.node);
            self.publish(ClusterEvent::Failed(member.clone()));
        }
    }

    /// A restarted node replaced its previous incarnation.
    pub(super) fn member_renamed(&self, before: &Member, after: Member) {
        let mut state = self.write();
        if state.members.get(&before.node) == Some(before) {
            state.members.insert(after.node.clone(), after.clone());
            state.unreachable.remove(&after.node);
            self.publish(ClusterEvent::Failed(before.clone()));
            self.publish(ClusterEvent::Up(after));
        }
    }
}
