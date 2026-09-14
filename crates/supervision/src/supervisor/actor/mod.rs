use std::{
    sync::Arc,
    task::{Context, Poll},
};

use crate::_prelude::*;
use futures::{Stream, StreamExt};
use indexmap::{IndexMap, IndexSet};
use rootcause::{Report, prelude::ResultExt as _, report};
use zestors_runtime::{ActorStatus, errors::DuplicatePidError};

pub struct Supervisor {
    supervisees: SuperviseeMap,
    strategy: SupervisionStrategy,
    restart_intensity: RestartIntensity,
    source: Option<Arc<dyn SupervisorSource>>,
}

impl Actor for Supervisor {
    type Interface = SupervisorInterface;
    type Exit = ();

    async fn run(self, inbox: Inbox<Self::Interface>) -> Result<Self::Exit, Report> {
        SupervisorActor::new(self, inbox).run().await
    }
}

impl Supervisor {
    pub fn blueprint() -> SupervisorBlueprint {
        SupervisorBlueprint::new()
    }

    fn new(
        supervisees: SuperviseeMap,
        strategy: SupervisionStrategy,
        restart_intensity: RestartIntensity,
        source: Option<Arc<dyn SupervisorSource>>,
    ) -> Self {
        Self {
            supervisees,
            strategy,
            restart_intensity,
            source,
        }
    }
}

struct SupervisorActor {
    supervisees: SuperviseeMap,
    inbox: Inbox<SupervisorInterface>,
    source: Option<Arc<dyn SupervisorSource>>,
    restarter: RestartLimiter,
    strategy: SupervisionStrategy,
    /// The processes that are still initializing
    initializing: IndexSet<Pid>,
    exiting: IndexSet<Pid>,
}

impl SupervisorActor {
    fn new(supervisor: Supervisor, inbox: Inbox<SupervisorInterface>) -> Self {
        Self {
            supervisees: supervisor.supervisees,
            source: supervisor.source,
            inbox,
            restarter: RestartLimiter::new(supervisor.restart_intensity),
            strategy: supervisor.strategy,
            initializing: IndexSet::new(),
            exiting: IndexSet::new(),
        }
    }

    async fn start(&mut self) -> Result<(), Report> {
        self.inbox.set_manual_init();

        if let Some(source) = &self.source {
            for spec in source
                .load_all()
                .await
                .attach("Failed to load supervisees from source")?
            {
                self.supervisees
                    .add(Supervisee::new(spec))
                    .attach("Failed to add supervisee from source")?;
            }
        }

        self.supervisees
            .start_all()
            .map_err(|(pid, e)| report!(e).attach(pid))?;

        self.initializing = self.supervisees.pids().cloned().collect();

        Ok(())
    }

    async fn run(&mut self) -> Result<(), Report> {
        self.start().await?;

        while let Some(next) = self.next().await {
            let flow = match next {
                InnerNext::Inbox(InboxEvent::Message(msg)) => self.handle_msg(msg),
                InnerNext::Inbox(InboxEvent::Signal(signal)) => self.handle_signal(signal),
                InnerNext::Source(ev) => self.handle_source_event(ev),
                InnerNext::Supervisee(ev) => self.handle_supervisee_event(ev),
            };

            match flow {
                Flow::Continue => continue,
                Flow::ExitNormal => return Ok(()),
            }
        }

        Ok(())
    }

    fn handle_supervisee_event(&mut self, SuperviseeNext { pid, item }: SuperviseeNext) -> Flow {
        match item {
            SuperviseeItem::StartError(_e) => {
                self.initializing.swap_remove(&pid);
                self.trigger_child_restart(pid);
            }

            SuperviseeItem::Exit(exit) => {
                if self.exiting.swap_remove(&pid) && self.exiting.is_empty() {
                    if self.is_exiting() {
                        return Flow::ExitNormal;
                    } else {
                        let restarting = self.supervisees.restart_all();
                        self.initializing.extend(restarting);
                    }
                }

                let supervisee = self.get_supervisee(&pid).expect("Should exist");

                if supervisee.requires_restart(&exit) {
                    self.trigger_child_restart(pid);
                } else {
                    self.remove_pid(&pid);
                }
            }

            SuperviseeItem::Spawned => {}

            SuperviseeItem::Initialized => {
                if self.initializing.swap_remove(&pid)
                    && self.is_initializing()
                    && self.initializing.is_empty()
                {
                    self.inbox.register_initialized();
                }
            }
        };

        Flow::Continue
    }

    fn trigger_child_restart(&mut self, pid: Pid) {
        if self.is_exiting() {
            return;
        }

        let supervisee = self.get_supervisee(&pid).unwrap();

        // If the supervisee is no longer dead, it was already fixed by
        // another restart-event
        if !matches!(supervisee.status(), SuperviseeStatus::Dead) {
            return;
        }

        let pids = match self.strategy {
            SupervisionStrategy::OneForOne => vec![pid],
            SupervisionStrategy::OneForAll => self.supervisees.pids().cloned().collect(),
        };

        let exiting_pids = match self.supervisees.stop_pids(pids) {
            Ok(pids) => pids,
            Err(_e) => {
                self.shutdown();
                return;
            }
        };

        self.exiting.extend(exiting_pids)
    }

    fn handle_signal(&mut self, signal: Signal) -> Flow {
        match signal {
            Signal::Shutdown => {
                self.shutdown();
            }
            Signal::Suspend | Signal::Resume => (),
        };

        Flow::Continue
    }

    fn handle_msg(&mut self, msg: SupervisorInterface) -> Flow {
        match msg {
            SupervisorInterface::Children(envelope) => {
                envelope.reply(self.child_descriptions()).ok();
            }
            SupervisorInterface::Health(envelope) => {
                envelope.reply(self.health()).ok();
            }
            SupervisorInterface::Register(Envelope { msg, request }) => {
                let res = self.add_spec(msg.0);
                request.reply(res).ok();
            }
            SupervisorInterface::Deregister(Envelope { msg, request }) => {
                let supervisee = self.remove_pid(&msg.0);
                request.reply(supervisee).ok();
            }
        };

        Flow::Continue
    }

    fn handle_source_event(&mut self, ev: SupervisorSourceEvent) -> Flow {
        match ev {
            SupervisorSourceEvent::Added(spec) => {
                if let Err(error) = self.add_spec(spec) {
                    tracing::error!(%error, "Failed to add spec from source. Already registered");
                }
            }

            SupervisorSourceEvent::Removed(pid) => {
                let _supervisee = self.remove_pid(&pid);
            }
        };

        Flow::Continue
    }

    fn add_spec(&mut self, spec: ChildSpec) -> Result<(), DuplicatePidError> {
        let supervisee = Supervisee::new(spec);
        let pid = supervisee.pid().clone();
        self.supervisees.add(supervisee)?;

        if !self.is_exiting() {
            self.supervisees.get_mut(&pid).unwrap().start().unwrap();
            self.initializing.insert(pid);
        }

        Ok(())
    }

    fn remove_pid(&mut self, pid: &Pid) -> Option<Supervisee> {
        let supervisee = self.supervisees.remove(&pid);

        if supervisee.is_some() {
            self.initializing.swap_remove(pid);
        }

        supervisee
    }

    fn shutdown(&mut self) {
        self.exiting.extend(
            self.supervisees
                .addresses()
                .filter(|a| !a.is_dead())
                .map(|a| a.pid().clone()),
        );

        self.supervisees.stop_all();
    }

    fn is_initializing(&self) -> bool {
        matches!(self.inbox.status(), ActorStatus::Initializing)
    }

    fn is_initialized(&self) -> bool {
        matches!(
            self.inbox.status(),
            ActorStatus::Running | ActorStatus::Suspended
        )
    }

    fn is_exiting(&self) -> bool {
        matches!(self.inbox.status(), ActorStatus::Stopping)
    }

    fn child_descriptions(&self) -> Vec<ChildDescription> {
        self.supervisees.child_descriptions()
    }

    fn health(&self) -> Health {
        HealthStatus::Healthy.into_health()
    }

    fn get_supervisee(&self, pid: &Pid) -> Option<&Supervisee> {
        self.supervisees.get(pid)
    }

    fn get_supervisee_mut(&mut self, pid: &Pid) -> Option<&mut Supervisee> {
        self.supervisees.get_mut(pid)
    }

    async fn next(&mut self) -> Option<InnerNext> {
        tokio::select! {
            biased;

            Some(ev) = self.inbox.recv_event() => {
                Some(InnerNext::Inbox(ev))
            }

            Some(ev) = async { match &self.source {
                Some(source) => source.next().await,
                None => None,
            } } => {
                Some(InnerNext::Source(ev))
            }

            Some(ev) = self.supervisees.next() => {
                Some(InnerNext::Supervisee(ev))
            }
        }
    }
}

enum InnerNext {
    Inbox(InboxEvent<SupervisorInterface>),
    Source(SupervisorSourceEvent),
    Supervisee(SuperviseeNext),
}

enum Flow {
    Continue,
    ExitNormal,
}

#[derive(Debug, thiserror::Error)]
pub enum SupervisorExitError {
    #[error("Initialization failed")]
    InitializationError,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum SupervisorStatus {
    Initializing,
    Running,
    ShuttingDown,
}

mod map;
use map::SuperviseeMap;

mod interface;
pub use interface::*;

mod blueprint;
pub use blueprint::*;
