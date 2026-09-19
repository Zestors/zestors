use crate::{
    _prelude::*,
    actor::{
        one_for_all::OneForAllSupervisor, one_for_one::OneForOneSupervisor,
        rest_for_one::RestForOneSupervisor,
    },
};
use futures::{Stream, StreamExt};
use indexmap::{IndexMap, IndexSet};
use map::SuperviseeMap;
use rootcause::{Report, prelude::ResultExt as _, report};
use std::{
    ops::ControlFlow,
    sync::Arc,
    task::{Context, Poll},
};
use zestors_runtime::{ActorStatus, ExitStatus, errors::DuplicateNameError};
use zestors_supervision::{
    RestartIntensity,
    messages::{Health, HealthStatus},
};

/// A supervisor actor: owns a set of [`ChildSpec`] supervisees, starts them,
/// watches them, and restarts them according to its [`SupervisionStrategy`]
/// and restart budget when they exit.
///
/// Constructed declaratively from a [`SupervisorBlueprint`] via
/// [`Supervisor::blueprint`], or by implementing [`Blueprint`] for a
/// custom builder.
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
            restarter: RestartLimiter::new(self.restart_intensity),
        }
        .run(self.strategy)
        .await
    }
}

impl Supervisor {
    /// Creates a new [`SupervisorBlueprint`] (defaulting to
    /// [`SupervisionStrategy::OneForOne`], no children) for building a
    /// [`Supervisor`] declaratively.
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
    restarter: RestartLimiter,
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
        matches!(self.inbox.status(), ActorStatus::Exiting)
    }

    fn health(&self) -> Health {
        HealthStatus::Healthy.into_health()
    }

    async fn next(&mut self) -> Option<InnerNext> {
        tokio::select! {
            biased;

            Some(ev) = self.inbox.recv_event_always() => {
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

    fn add_spec(&mut self, spec: ChildSpec) -> Result<(), DuplicateNameError> {
        if self.is_exiting() {
            tracing::warn!("Attempted to add a child spec while supervisor is exiting");
            return Ok(());
        }

        let supervisee = Supervisee::new(spec);
        let name = supervisee.name().clone();
        self.supervisees.add(supervisee)?;
        self.supervisees
            .get_mut(&name)
            .expect("Just inserted")
            .start()
            .expect("Supervisee should not be shutting down.");

        Ok(())
    }

    /// Shared by `one_for_one` and `one_for_all`: pulls this supervisor's own
    /// inbox into `Exiting` and stops every currently-alive supervisee all
    /// at once, returning `Break` once there's nothing left to wait for.
    /// `rest_for_one` doesn't use this — its children can depend on each
    /// other, so it tears them down one at a time instead (see
    /// `RestForOneSupervisor::shutdown`).
    pub(super) fn shutdown(&mut self, exiting: &mut IndexSet<Name>) -> ControlFlow<()> {
        self.register_exiting();

        exiting.extend(self.supervisees.stop_all());

        if exiting.is_empty() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    /// Pulls this supervisor's own inbox into `Exiting`, so e.g. `add_spec`
    /// starts rejecting new children and external watchers see it exiting.
    pub(super) fn register_exiting(&mut self) {
        self.inbox.register_exiting();
    }

    pub(super) fn handle_initialized(&mut self, initializing: &mut IndexSet<Name>, name: &Name) {
        if initializing.swap_remove(name) && self.is_initializing() && initializing.is_empty() {
            self.inbox.register_initialized();
        }
    }

    /// The part of `remove_spec` common to every strategy; strategies with extra
    /// per-name bookkeeping (e.g. an in-progress restart cascade) clean that up
    /// themselves around this call.
    pub(super) fn remove_spec(
        &mut self,
        initializing: &mut IndexSet<Name>,
        exiting: &mut IndexSet<Name>,
        name: &Name,
    ) -> Option<Supervisee> {
        let supervisee = self.supervisees.remove(name);

        if supervisee.is_some() {
            initializing.swap_remove(name);
            exiting.swap_remove(name);
        }

        supervisee
    }
}

enum InnerNext {
    Inbox(InboxEvent<SupervisorInterface>),
    Source(SupervisorSourceEvent),
    Supervisee(SuperviseeNext),
}

enum ExitReason {
    /// The supervisee failed to start; always requires a restart attempt.
    StartFailure,
    /// The supervisee exited before finishing initialization.
    InitExit(ExitStatus),
    Exit(SuperviseeExit),
}

impl ExitReason {
    fn requires_restart(&self, supervisee: &Supervisee) -> bool {
        match self {
            ExitReason::StartFailure => supervisee.cfg().restart_mode != RestartMode::Never,
            ExitReason::InitExit(status) => supervisee.requires_restart_for_init_exit(status),
            ExitReason::Exit(e) => supervisee.requires_restart(e),
        }
    }
}

mod map;
mod one_for_all;
mod one_for_one;
mod rest_for_one;

mod runner;
use runner::Strategy;

mod supervisee;
use supervisee::*;

mod interface;
pub use interface::*;

mod blueprint;
pub use blueprint::*;
