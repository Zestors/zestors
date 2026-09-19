use super::{Cluster, ClusterEvent, Member, State};
use crate::NodeId;

/// Every change to the cluster's membership goes through here, and is published
/// while holding the state lock, so that
/// [`Cluster::subscribe_with_snapshot`], which subscribes under the same lock,
/// sees each change either in its snapshot or as an event, never both or neither.
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
    pub(super) fn member_unreachable(&self, node: &NodeId) -> Option<Member> {
        let mut state = self.write();
        let member = state.members.get(node)?.clone();
        state.unreachable.insert(node.clone()).then(|| {
            self.publish(ClusterEvent::Unreachable(member.clone()));
            member
        })
    }

    /// This node can connect to `node` again. Returns the member if it had been
    /// reported unreachable.
    pub(super) fn member_reachable(&self, node: &NodeId) -> Option<Member> {
        let mut state = self.write();
        let member = state.members.get(node)?.clone();
        state.unreachable.remove(node).then(|| {
            self.publish(ClusterEvent::Reachable(member.clone()));
            member
        })
    }

    /// The node said goodbye. Returns it if it was known in that generation.
    pub(super) fn member_left(&self, node: &NodeId, generation: u64) -> Option<Member> {
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
