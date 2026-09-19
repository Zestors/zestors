use super::{
    Command,
    message::{Message, sender_of},
};
use crate::{
    ClusterTimings, Member, NodeId, NodeStatus,
    cluster::{
        Cluster,
        link::{Incoming, Links, PeerEvent, Protocol},
    },
};
use foca::{
    AccumulatingRuntime, Config, Foca, NoCustomBroadcast, OwnedNotification, PostcardCodec, Timer,
};
use rand::rngs::StdRng;
use std::collections::HashMap;
use tokio::{
    sync::{broadcast, mpsc},
    time::{MissedTickBehavior, interval},
};
use tokio_util::time::{DelayQueue, delay_queue};

pub(super) struct Driver {
    foca: Foca<Member, PostcardCodec, StdRng, NoCustomBroadcast>,
    runtime: AccumulatingRuntime<Member>,
    cluster: Cluster,
    links: Links,
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
    pub(super) fn new(
        cluster: Cluster,
        config: Config,
        rng: StdRng,
        links: Links,
        seeds: Vec<Member>,
        timings: ClusterTimings,
    ) -> Self {
        Self {
            foca: Foca::new(cluster.local(), config, rng, PostcardCodec),
            cluster,
            links,
            seeds,
            timings,
            leaving: false,
            timers: DelayQueue::new(),
            pending_down: DelayQueue::new(),
            pending_keys: HashMap::new(),
            runtime: AccumulatingRuntime::new(),
        }
    }

    pub(super) async fn run(
        mut self,
        mut inbox: mpsc::Receiver<Incoming>,
        mut peers: broadcast::Receiver<PeerEvent>,
        mut commands: mpsc::Receiver<Command>,
    ) {
        let mut rejoin = interval(self.timings.seed_retry);
        rejoin.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                Some(message) = inbox.recv() => self.on_incoming(message),
                // A receiver that fell behind just misses some; the next report still counts.
                Ok(event) = peers.recv() => self.on_peer_event(event),
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
                            self.send(&member, Message::Departure);
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

    fn on_peer_event(&mut self, event: PeerEvent) {
        match event {
            PeerEvent::Unreachable(node) => {
                if let Some(member) = self.cluster.member_unreachable(&node) {
                    tracing::warn!(node = %member.node, addr = %member.addr, "Node unreachable");
                }
            }
            PeerEvent::Reachable(node) => {
                if let Some(member) = self.cluster.member_reachable(&node) {
                    tracing::info!(node = %member.node, addr = %member.addr, "Node reachable again");
                }
            }
            // Nothing here rides on a single connection: a lost one is
            // replaced by the next message, and whether the node is still up
            // is for the failure detector to say.
            PeerEvent::Disconnected { node, generation } => {
                tracing::debug!(%node, generation, "Connection to node lost");
            }
        }
    }

    /// Queues `message` for `to`. Membership tolerates loss, so a full queue
    /// just drops it.
    fn send(&self, to: &Member, message: Message) {
        let sender = self
            .links
            .sender(to, Protocol::MEMBERSHIP, message.delivery(), 0);
        if sender.try_send(message.encode()).is_err() {
            tracing::debug!(node = %to.node, "Peer queue full or closed, dropping message");
        }
    }

    fn on_incoming(&mut self, message: Incoming) {
        let Some(decoded) = Message::decode(message.payload) else {
            tracing::debug!(node = %message.from, "Dropping malformed membership message");
            return;
        };
        match decoded {
            Message::Gossip(data) => {
                // Whoever the packet says it is from must be who sent it.
                if sender_of(&data).as_ref() != Some(&message.from) {
                    tracing::warn!(node = %message.from, "Dropping gossip that names another sender");
                    return;
                }
                if let Err(err) = self.foca.handle_data(&data, &mut self.runtime) {
                    tracing::debug!("Failed to handle gossip message: {err}");
                }
            }
            Message::Departure => {
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
            self.send(&to, Message::Gossip(data));
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
                self.links.forget(&member.node, member.generation);
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
                self.links.forget(&before.node, before.generation);
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

#[cfg(all(test, feature = "sim"))]
mod tests {
    use super::*;
    use crate::{
        Cluster, NodeAddr,
        backend::LocalNode,
        cluster::{link::Starter, sim::SimNetwork},
    };
    use foca::Foca;
    use rand::SeedableRng;

    fn member(name: &str) -> Member {
        Member {
            node: NodeId::new(name),
            addr: NodeAddr::new(format!("{name}:7000")),
            generation: 1,
        }
    }

    /// A driver for node-a, over a simulated network, that has heard nothing yet.
    async fn driver() -> Driver {
        let network = SimNetwork::new(1);
        let (links, _) = Starter::new(network.backend("node-a:7000"))
            .start(
                LocalNode {
                    id: NodeId::new("node-a"),
                    generation: 1,
                },
                ClusterTimings::default(),
            )
            .await
            .unwrap();
        Driver::new(
            Cluster::new(member("node-a")),
            Config::simple(),
            StdRng::seed_from_u64(1),
            links,
            Vec::new(),
            ClusterTimings::default(),
        )
    }

    /// What node-b sends node-a when it wants to join.
    fn announcement_from_node_b() -> Message {
        let mut foca = Foca::new(
            member("node-b"),
            Config::simple(),
            StdRng::seed_from_u64(2),
            PostcardCodec,
        );
        let mut runtime = AccumulatingRuntime::new();
        foca.announce(member("node-a"), &mut runtime).unwrap();
        Message::Gossip(runtime.to_send().expect("An announcement").1)
    }

    fn arrives_from(sender: &str) -> Incoming {
        Incoming {
            from: NodeId::new(sender),
            generation: 1,
            payload: announcement_from_node_b().encode(),
        }
    }

    /// node-a answers an announcement with what it knows; whether it does tells
    /// whether it took the packet in.
    #[tokio::test]
    async fn gossip_is_taken_from_the_node_it_names() {
        let mut driver = driver().await;
        driver.on_incoming(arrives_from("node-b"));
        assert!(driver.runtime.to_send().is_some());
    }

    #[tokio::test]
    async fn gossip_naming_another_sender_is_dropped() {
        let mut driver = driver().await;
        // The packet says node-b, but the backend says it came from mallory.
        driver.on_incoming(arrives_from("mallory"));
        assert!(driver.runtime.to_send().is_none());
    }
}
