use std::{
    sync::Arc,
    task::{Context, Poll},
};

use crate::{
    _prelude::*,
    supervisor::actor::{
        one_for_all::OneForAllSupervisor, one_for_one::OneForOneSupervisor,
        rest_for_one::RestForOneSupervisor,
    },
};
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
        SupervisorInner {
            supervisees: self.supervisees,
            inbox,
            source: self.source,
        }
        .run(self.strategy)
        .await
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

struct SupervisorInner {
    supervisees: SuperviseeMap,
    inbox: Inbox<SupervisorInterface>,
    source: Option<Arc<dyn SupervisorSource>>,
}

impl SupervisorInner {
    async fn run(&mut self, strategy: SupervisionStrategy) -> Result<(), Report> {
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

        match strategy {
            SupervisionStrategy::OneForOne => {
                OneForOneSupervisor::new(self).run().await?;
            }
            SupervisionStrategy::OneForAll => {
                OneForAllSupervisor::new(self).run().await?;
            }
            SupervisionStrategy::RestForOne => {
                RestForOneSupervisor::new(self).run().await?;
            }
        }

        Ok(())
    }

    fn is_initializing(&self) -> bool {
        matches!(self.inbox.status(), ActorStatus::Initializing)
    }

    fn is_exiting(&self) -> bool {
        matches!(self.inbox.status(), ActorStatus::Stopping)
    }

    fn health(&self) -> Health {
        HealthStatus::Healthy.into_health()
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

    fn add_spec(&mut self, spec: ChildSpec) -> Result<(), DuplicatePidError> {
        if self.is_exiting() {
            tracing::warn!("Attempted to add a child spec while supervisor is exiting");
            return Ok(());
        }

        let supervisee = Supervisee::new(spec);
        let pid = supervisee.pid().clone();
        self.supervisees.add(supervisee)?;
        self.supervisees
            .get_mut(&pid)
            .expect("Just inserted")
            .start()
            .expect("Supervisee should not be shutting down.");

        Ok(())
    }
}

enum InnerNext {
    Inbox(InboxEvent<SupervisorInterface>),
    Source(SupervisorSourceEvent),
    Supervisee(SuperviseeNext),
}

enum ExitReason {
    /// The supervisee failed to start; always requires a restart attempt.
    Start,
    Exit(SuperviseeExit),
}

impl ExitReason {
    fn requires_restart(&self, supervisee: &Supervisee) -> bool {
        match self {
            ExitReason::Start => true,
            ExitReason::Exit(e) => supervisee.requires_restart(e),
        }
    }
}

mod map;
mod one_for_all;
mod one_for_one;
mod rest_for_one;
mod shared;
use map::SuperviseeMap;

mod interface;
pub use interface::*;

mod blueprint;
pub use blueprint::*;
