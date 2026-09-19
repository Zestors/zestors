//! Keeps track of who is in the cluster: the membership protocol ([`foca`])
//! running over a [`Net`].

mod driver;

use super::{
    Cluster, Member, NodeStatus,
    net::{Event, Net},
};
use crate::{ClusterTimings, Seed};
use driver::Driver;
use foca::Config;
use rand::rngs::StdRng;
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

enum Command {
    Announce(Member),
    Leave(oneshot::Sender<()>),
}

/// What the membership protocol runs with.
pub(super) struct Options {
    /// The protocol's own settings, which decide how quickly failures are detected.
    pub(super) foca: Config,
    /// The nodes to contact when joining.
    pub(super) seeds: Vec<Seed>,
    pub(super) timings: ClusterTimings,
    /// Decides the protocol's random choices.
    pub(super) rng: StdRng,
}

/// Handle to the task running the membership protocol.
pub(super) struct Membership {
    commands: mpsc::Sender<Command>,
    task: JoinHandle<()>,
    leave_grace: Duration,
}

impl Membership {
    /// Makes `local` part of the cluster over `net`: marks `cluster` as up,
    /// starts the protocol and announces to the seeds. `events` is what `net`
    /// received.
    pub(super) async fn start(
        cluster: Cluster,
        local: Member,
        options: Options,
        net: Arc<dyn Net>,
        events: mpsc::Receiver<Event>,
    ) -> Self {
        cluster.set_local(local);
        cluster.set_status(NodeStatus::Up);

        let seeds: Vec<Member> = options
            .seeds
            .iter()
            .map(|seed| Member {
                node: seed.node.clone(),
                addr: seed.addr,
                generation: 0,
            })
            .collect();
        let leave_grace = options.timings.leave_grace;
        let (commands, command_rx) = mpsc::channel(16);
        let task = tokio::spawn(
            Driver::new(
                cluster,
                options.foca,
                options.rng,
                net,
                seeds.clone(),
                options.timings,
            )
            .run(events, command_rx),
        );
        let membership = Self {
            commands,
            task,
            leave_grace,
        };
        for seed in seeds {
            membership.announce(seed).await;
        }
        membership
    }

    /// Asks the node at `seed` to let us join the cluster.
    async fn announce(&self, seed: Member) {
        let _ = self.commands.send(Command::Announce(seed)).await;
    }

    /// Tells the cluster this node is leaving, then stops the protocol.
    pub(super) async fn leave(mut self) {
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
