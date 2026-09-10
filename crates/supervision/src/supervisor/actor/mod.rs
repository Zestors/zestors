use std::task::{Context, Poll};

use futures::{
    Stream, StreamExt,
    future::{self, BoxFuture, join_all},
};
use indexmap::IndexMap;
use rootcause::{
    Report,
    prelude::{IteratorExt as _, ResultExt as _},
    report,
};

use crate::_prelude::*;

pub struct Supervisor {
    blueprint: SupervisorBlueprint,
}

impl Supervisor {
    pub fn blueprint() -> SupervisorBlueprint {
        SupervisorBlueprint::new()
    }
}

impl Actor for Supervisor {
    type Interface = SupervisorInterface;
    type Exit = ();

    async fn run(self, state: Inbox<Self::Interface>) -> Result<Self::Exit, Report> {
        todo!()
    }
}

#[allow(unreachable_code)]
pub async fn run_supervisor(supervisor: BaseSupervisor) -> Result<(), Report> {
    match supervisor.start() {
        Ok(init) => match init.run().await {
            Ok(running) => match running.run().await {
                Ok(()) => Ok(()),
                Err(exit) => exit.run().await,
            },
            Err(exit) => exit.run().await,
        },
        Err(exit) => exit.run().await,
    }
}

pub struct BaseSupervisor {
    supervisees: SuperviseeMap,
    strategy: SupervisionStrategy,
    restart_limiter: RestartLimiter,
    inbox: Inbox<SupervisorInterface>,
}

impl BaseSupervisor {
    fn start(mut self) -> Result<InitializingSupervisor, ExitingSupervisor> {
        self.inbox.set_manual_init();

        // Start all supervisees
        match self.supervisees.start_all() {
            Ok(()) => Ok(InitializingSupervisor::new(self)),
            Err((pid, e)) => {
                tracing::error!("Failed to start supervisee {pid}: {e}");
                tracing::info!("Stopping all supervisees due to failure");

                self.stop_supervisees();
                Err(ExitingSupervisor::new(self))
            }
        }
    }

    fn stop_supervisees(&mut self) {
        self.inbox.register_stopping();
        self.supervisees.stop_all();
    }

    fn create_initialization_future(&mut self) -> BoxFuture<'static, Result<(), Report>> {
        let addressses = self.supervisees.addresses().cloned().collect::<Vec<_>>();

        Box::pin(async move {
            join_all(
                addressses
                    .into_iter()
                    .map(|addr| async move { (addr.watch_initialization().await, addr) }),
            )
            .await
            .into_iter()
            .map(|(res, addr)| match res {
                Ok(()) => Ok(()),
                Err(exit) => exit
                    .into_result()
                    .attach(format!("Child {} failed to initialize", addr.pid())),
            })
            .collect_reports()
            .map_err(|e| report!("One or more children failed to start").attach(e))
        })
    }

    fn create_exit_watcher(&mut self) -> BoxFuture<'static, Result<(), Report>> {
        let addressses = self.supervisees.addresses().cloned().collect::<Vec<_>>();

        Box::pin(async move {
            join_all(
                addressses
                    .into_iter()
                    .map(|addr| async move { (addr.watch_exit().await, addr) }),
            )
            .await
            .into_iter()
            .map(|(res, addr)| match res {
                Ok(()) => Ok(()),
                Err(exit) => {
                    Err(report!(exit).attach(format!("Child {} failed to exit", addr.pid())))
                }
            })
            .collect_reports()
            .map_err(|e| report!("One or more children failed to start").attach(e))
        })
    }

    fn handle_msg(&mut self, msg: SupervisorInterface) {
        match msg {
            SupervisorInterface::Children(Envelope { msg: _, handle }) => {
                let descriptions = self.supervisees.child_descriptions();
                handle.reply(descriptions).ok();
            }
            SupervisorInterface::Health(Envelope { msg: _, handle }) => {
                handle.reply(HealthStatus::Healthy.into_health()).ok();
            }
        }
    }

    fn allow_restart(&mut self) -> bool {
        self.restart_limiter.allow_restart()
    }

    fn get(&self, pid: &Pid) -> Option<&Supervisee> {
        self.supervisees.get(pid)
    }
}

struct InitializingSupervisor {
    supervisor: BaseSupervisor,
    init_watcher: BoxFuture<'static, Result<(), Report>>,
}

impl InitializingSupervisor {
    pub fn new(mut supervisor: BaseSupervisor) -> Self {
        let init_watcher = supervisor.create_initialization_future();

        Self {
            supervisor,
            init_watcher,
        }
    }

    pub async fn run(mut self) -> Result<RunningSupervisor, ExitingSupervisor> {
        loop {
            tokio::select! {
                biased;

                init_result = &mut self.init_watcher => {
                    match init_result {
                        Ok(()) => break Ok(RunningSupervisor::new(self.supervisor)),
                        Err(e) => {
                            tracing::error!("Supervisor initialization failed: {e:?}");
                            tracing::info!("Stopping all supervisees due to failure");

                            self.supervisor.stop_supervisees();
                            break Err(ExitingSupervisor::new(self.supervisor));
                        }
                    }
                }

                Some(msg) = self.supervisor.inbox.next() => {
                    match msg {
                        Event::Signal(signal) => match signal {
                            Signal::Shutdown => {
                                tracing::info!("Supervisor received shutdown signal during initialization");
                                self.supervisor.stop_supervisees();
                                break Err(ExitingSupervisor::new(self.supervisor));
                            }
                            Signal::Resume | Signal::Suspend => (),
                        },
                        Event::Message(msg) => {
                            self.supervisor.handle_msg(msg);
                        }
                    }
                 }
            };
        }
    }
}

struct ExitingSupervisor {
    supervisor: BaseSupervisor,
    exit_watcher: BoxFuture<'static, Result<(), Report>>,
}

impl ExitingSupervisor {
    pub fn new(mut supervisor: BaseSupervisor) -> Self {
        let exit_watcher = supervisor.create_exit_watcher();

        Self {
            supervisor,
            exit_watcher,
        }
    }

    pub async fn run(mut self) -> Result<(), Report> {
        loop {
            tokio::select! {
                biased;

                exit_result = &mut self.exit_watcher => {
                    break exit_result;
                }

                Some(msg) = self.supervisor.inbox.next() => {
                    match msg {
                        Event::Signal(signal) => match signal {
                            Signal::Shutdown
                            | Signal::Resume
                            | Signal::Suspend => (),
                        },

                        Event::Message(msg) => {
                            self.supervisor.handle_msg(msg);
                        }
                    }
                 }
            };
        }
    }
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
    Stopping,
}

mod running;
use running::RunningSupervisor;

mod map;
use map::SuperviseeMap;

mod interface;
pub use interface::*;

mod blueprint;
pub use blueprint::*;
