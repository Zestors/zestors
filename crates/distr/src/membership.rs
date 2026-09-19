use crate::{
    Cluster, ClusterEvent, ClusterTimings, NodeId, NodeStatus,
    net::{Event, Frame, Incoming, Net},
};
use foca::{
    AccumulatingRuntime, Config, Foca, Identity, NoCustomBroadcast, OwnedNotification,
    PostcardCodec, Timer,
};
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tokio_util::time::{DelayQueue, delay_queue};

/// A node in the cluster, as known to the membership protocol.
///
/// A node is identified by its [`NodeId`] alone; `addr` is only where it can
/// currently be reached and may change between restarts. The `generation`
/// distinguishes successive incarnations of the same node, so a restarted node
/// replaces its previous incarnation (like Erlang's `creation`, but carried by
/// the node rather than by pids).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Member {
    /// The node's name; must be a DNS name matching its TLS certificate.
    pub node: NodeId,
    /// The address peers currently reach the node on.
    pub addr: SocketAddr,
    /// Increases every time the node restarts.
    pub generation: u64,
}

impl Identity for Member {
    type Addr = NodeId;

    fn renew(&self) -> Option<Self> {
        Some(Member {
            generation: self.generation + 1,
            ..self.clone()
        })
    }

    fn addr(&self) -> NodeId {
        self.node.clone()
    }

    fn win_addr_conflict(&self, adversary: &Self) -> bool {
        self.generation > adversary.generation
    }
}

enum Command {
    Announce(Member),
    Leave(oneshot::Sender<()>),
}

/// Handle to the task running the membership protocol.
pub(crate) struct Membership {
    commands: mpsc::Sender<Command>,
    task: JoinHandle<()>,
    leave_grace: Duration,
}

impl Membership {
    /// Starts the membership protocol for `cluster`'s local node.
    pub(crate) fn start(
        cluster: Cluster,
        config: Config,
        seeds: Vec<Member>,
        timings: ClusterTimings,
        transport: Arc<dyn Net>,
        events: mpsc::Receiver<Event>,
        rng: StdRng,
    ) -> Self {
        let leave_grace = timings.leave_grace;
        let (commands, command_rx) = mpsc::channel(16);
        let task = tokio::spawn(
            Driver {
                foca: Foca::new(
                    cluster.local(),
                    config,
                    rng,
                    PostcardCodec,
                ),
                cluster,
                transport,
                seeds,
                timings,
                leaving: false,
                timers: DelayQueue::new(),
                pending_down: DelayQueue::new(),
                pending_keys: HashMap::new(),
                runtime: AccumulatingRuntime::new(),
            }
            .run(events, command_rx),
        );
        Self {
            commands,
            task,
            leave_grace,
        }
    }

    /// Asks the node at `seed` to let us join the cluster.
    pub(crate) async fn announce(&self, seed: Member) {
        let _ = self.commands.send(Command::Announce(seed)).await;
    }

    /// Tells the cluster this node is leaving, then stops the protocol.
    pub(crate) async fn leave(mut self) {
        let (tx, rx) = oneshot::channel();
        if self.commands.send(Command::Leave(tx)).await.is_ok() {
            let _ = tokio::time::timeout(self.leave_grace, rx).await;
        }
        let _ = (&mut self.task).await;
    }
}

impl Drop for Membership {
    /// Dropping the handle stops the protocol without saying goodbye.
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Driver {
    foca: Foca<Member, PostcardCodec, StdRng, NoCustomBroadcast>,
    runtime: AccumulatingRuntime<Member>,
    cluster: Cluster,
    transport: Arc<dyn Net>,
    seeds: Vec<Member>,
    timings: ClusterTimings,
    /// Set once we've announced our own departure, after which foca reports us as down.
    leaving: bool,
    /// Pending foca timers; owned by the driver, so they vanish with it.
    timers: DelayQueue<Timer<Member>>,
    /// Nodes foca declared down, waiting out [`ClusterTimings::departure_grace`] before being
    /// reported as failed.
    pending_down: DelayQueue<Member>,
    pending_keys: HashMap<NodeId, delay_queue::Key>,
}

impl Driver {
    async fn run(
        mut self,
        mut events: mpsc::Receiver<Event>,
        mut commands: mpsc::Receiver<Command>,
    ) {
        let mut rejoin = interval(self.timings.seed_retry);
        rejoin.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                Some(event) = events.recv() => self.on_event(event),
                Some(expired) = std::future::poll_fn(|cx| self.timers.poll_expired(cx)) => {
                    if let Err(err) = self.foca.handle_timer(expired.into_inner(), &mut self.runtime) {
                        tracing::debug!("Failed to handle timer: {err}");
                    }
                }
                Some(expired) = std::future::poll_fn(|cx| self.pending_down.poll_expired(cx)) => {
                    let member = expired.into_inner();
                    self.pending_keys.remove(&member.node);
                    self.cluster.member_failed(&member);
                }
                _ = rejoin.tick() => {
                    // Seeds may not have been up yet; keep trying until someone answers.
                    if self.foca.iter_members().next().is_none() {
                        self.announce_seeds();
                    }
                }
                command = commands.recv() => match command {
                    Some(Command::Announce(seed)) => self.announce(seed),
                    Some(Command::Leave(done)) => {
                        self.leaving = true;
                        self.cluster.set_status(NodeStatus::Leaving);
                        // Say goodbye directly first, so peers can tell this apart from a crash.
                        for member in self.cluster.members() {
                            self.transport.send(&member, Frame::Departure);
                        }
                        if let Err(err) = self.foca.leave_cluster(&mut self.runtime) {
                            tracing::debug!("Failed to leave cluster: {err}");
                        }
                        self.drain();
                        let _ = done.send(());
                        return;
                    }
                    None => return,
                },
            }
            self.drain();
        }
    }

    fn on_event(&mut self, event: Event) {
        match event {
            Event::Received(message) => self.on_incoming(message),
            Event::Unreachable(node) => {
                if let Some(member) = self.cluster.member_unreachable(&node) {
                    tracing::warn!(node = %member.node, addr = %member.addr, "Node unreachable");
                }
            }
            Event::Reachable(node) => {
                if let Some(member) = self.cluster.member_reachable(&node) {
                    tracing::info!(node = %member.node, addr = %member.addr, "Node reachable again");
                }
            }
        }
    }

    fn on_incoming(&mut self, message: Incoming) {
        match message.frame {
            Frame::Gossip(data) => {
                if let Err(err) = self.foca.handle_data(&data, &mut self.runtime) {
                    tracing::debug!("Failed to handle gossip message: {err}");
                }
            }
            Frame::Departure => {
                if let Some(member) = self.cluster.member_left(&message.from, message.generation) {
                    tracing::info!(node = %member.node, addr = %member.addr, "Node left");
                    self.cancel_pending(&member.node);
                }
            }
        }
    }

    /// Forgets a node's pending failure report.
    fn cancel_pending(&mut self, node: &NodeId) {
        if let Some(key) = self.pending_keys.remove(node) {
            self.pending_down.try_remove(&key);
        }
    }

    fn announce_seeds(&mut self) {
        for seed in self.seeds.clone() {
            self.announce(seed);
        }
    }

    fn announce(&mut self, seed: Member) {
        if seed.node == self.cluster.local().node {
            return;
        }
        if let Err(err) = self.foca.announce(seed, &mut self.runtime) {
            tracing::debug!("Failed to announce to seed: {err}");
        }
    }

    /// Carries out what foca asked for while handling the last input.
    fn drain(&mut self) {
        while let Some((to, data)) = self.runtime.to_send() {
            self.transport.send(&to, Frame::Gossip(data));
        }

        while let Some((after, timer)) = self.runtime.to_schedule() {
            self.timers.insert(timer, after);
        }

        while let Some(notification) = self.runtime.to_notify() {
            self.notify(notification);
        }
    }

    fn notify(&mut self, notification: OwnedNotification<Member>) {
        match notification {
            OwnedNotification::MemberUp(member) => {
                tracing::info!(node = %member.node, addr = %member.addr, "Node up");
                self.cancel_pending(&member.node);
                self.cluster.member_up(member);
            }
            OwnedNotification::MemberDown(member) => {
                tracing::info!(node = %member.node, addr = %member.addr, "Node down");
                self.transport.forget(&member.node, member.generation);
                // It may yet turn out to have left cleanly; see `ClusterTimings::departure_grace`.
                if self.cluster.contains(&member) {
                    let key = self
                        .pending_down
                        .insert(member.clone(), self.timings.departure_grace);
                    if let Some(old) = self.pending_keys.insert(member.node, key) {
                        self.pending_down.try_remove(&old);
                    }
                }
            }
            OwnedNotification::Rename(before, after) => {
                self.cancel_pending(&before.node);
                self.transport.forget(&before.node, before.generation);
                self.cluster.member_renamed(&before, after);
            }
            OwnedNotification::Rejoin(new_identity) => {
                tracing::warn!(
                    generation = new_identity.generation,
                    "Declared down by the cluster; rejoined with a new generation"
                );
                self.cluster.set_local(new_identity);
            }
            OwnedNotification::Active => tracing::debug!("Joined the cluster"),
            OwnedNotification::Idle => tracing::debug!("No other active nodes in the cluster"),
            OwnedNotification::Defunct if self.leaving => {}
            OwnedNotification::Defunct => {
                self.cluster.set_status(NodeStatus::Defunct);
                tracing::error!("Declared down by the cluster and unable to rejoin")
            }
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }
}

/// Membership changes are published while holding the state lock, so that
/// [`Cluster::subscribe_with_snapshot`], which subscribes under the same lock,
/// sees each change either in its snapshot or as an event, never both or neither.
impl Cluster {
    fn contains(&self, member: &Member) -> bool {
        self.shared
            .state
            .read()
            .expect("Not poisoned")
            .members
            .get(&member.node)
            == Some(member)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, crate::cluster::State> {
        self.shared.state.write().expect("Not poisoned")
    }

    fn publish(&self, event: ClusterEvent) {
        let _ = self.shared.events.send(event);
    }

    fn member_up(&self, member: Member) {
        let mut state = self.write();
        let previous = state.members.insert(member.node.clone(), member.clone());
        if previous.as_ref() != Some(&member) {
            state.unreachable.remove(&member.node);
            self.publish(ClusterEvent::Up(member));
        }
    }

    /// This node can't connect to `node`. Returns the member if that is news.
    fn member_unreachable(&self, node: &NodeId) -> Option<Member> {
        let mut state = self.write();
        let member = state.members.get(node)?.clone();
        state.unreachable.insert(node.clone()).then(|| {
            self.publish(ClusterEvent::Unreachable(member.clone()));
            member
        })
    }

    /// This node can connect to `node` again. Returns the member if it had been
    /// reported unreachable.
    fn member_reachable(&self, node: &NodeId) -> Option<Member> {
        let mut state = self.write();
        let member = state.members.get(node)?.clone();
        state.unreachable.remove(node).then(|| {
            self.publish(ClusterEvent::Reachable(member.clone()));
            member
        })
    }

    /// The node said goodbye. Returns it if it was known in that generation.
    fn member_left(&self, node: &NodeId, generation: u64) -> Option<Member> {
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
    fn member_failed(&self, member: &Member) {
        let mut state = self.write();
        if state.members.get(&member.node) == Some(member) {
            state.members.remove(&member.node);
            state.unreachable.remove(&member.node);
            self.publish(ClusterEvent::Failed(member.clone()));
        }
    }

    /// A restarted node replaced its previous incarnation.
    fn member_renamed(&self, before: &Member, after: Member) {
        let mut state = self.write();
        if state.members.get(&before.node) == Some(before) {
            state.members.insert(after.node.clone(), after.clone());
            state.unreachable.remove(&after.node);
            self.publish(ClusterEvent::Failed(before.clone()));
            self.publish(ClusterEvent::Up(after));
        }
    }
}
