use crate::_prelude::*;
use crate::ChildSpec;
use futures::{Stream, future};
use rootcause::Report;
use std::{
    fmt::Debug,
    pin::{Pin, pin},
    task::{Context, Poll},
};
use tokio::time::{error::Elapsed, timeout};
use zestors_runtime::{
    ActorOps, ActorStatus, Channel, Child, Dyn, ExitingChild, Pid,
    errors::{JoinError, StartOnError},
};

#[derive(Debug)]
pub struct Supervisee {
    spec: ChildSpec,
    state: SuperviseeState,
}

impl Supervisee {
    pub fn new(spec: ChildSpec) -> Self {
        Self {
            spec,
            state: SuperviseeState::idle(),
        }
    }

    pub fn get_description(&self) -> ChildDescription {
        ChildDescription {
            pid: self.pid().clone(),
            cfg: self.spec.cfg().clone(),
        }
    }

    pub fn status(&self) -> SuperviseeStatus {
        self.state.status()
    }

    pub fn start(&mut self) -> Result<bool, SuperviseeStartError> {
        self.state.map(|state| match state {
            // If the supervisee is idle or dead, we can start it.
            SuperviseeState::Dead { .. } => {
                let spec = self.spec.clone();
                let fut = StartFuture::new(async move {
                    Ok(timeout(spec.cfg().start_timeout, spec.start()).await??)
                });

                (SuperviseeState::Starting { fut }, Ok(true))
            }

            // If the supervisee is starting or alive, all is good
            SuperviseeState::Starting { .. } | SuperviseeState::Alive { .. } => (state, Ok(false)),

            // If the supervisee is shutting down, we cannot start it.
            SuperviseeState::ShuttingDown { .. } => {
                (state, Err(SuperviseeStartError::IsShuttingDown))
            }
        })
    }

    pub fn stop(&mut self) -> bool {
        self.state.map(|state| match state {
            // If the supervisee is dead, idle, or shutting down, we don't need to do anything.
            SuperviseeState::Dead { .. } | SuperviseeState::ShuttingDown { .. } => (state, false),

            // If the supervisee is starting, we drop the future and mark it as dead.
            SuperviseeState::Starting { fut } => {
                drop(fut);
                (SuperviseeState::start_cancelled(), true)
            }

            // If the supervisee is alive, we initiate a shutdown.
            SuperviseeState::Alive { child } => {
                let child = child.into_shutdown(self.spec.cfg().abort_timeout);
                (SuperviseeState::ShuttingDown { child }, true)
            }
        })
    }

    pub async fn supervise(&mut self) -> SuperviseeItem {
        match &mut self.state {
            SuperviseeState::Dead { .. } => future::pending().await,

            SuperviseeState::Starting { fut } => match fut.await {
                Ok(child) => {
                    self.state = SuperviseeState::Alive { child };
                    SuperviseeItem::Started
                }

                Err(start_error) => {
                    self.state = SuperviseeState::start_error();
                    SuperviseeItem::StartError(start_error)
                }
            },

            SuperviseeState::Alive { child } => match child.await {
                Ok(()) => {
                    self.state = SuperviseeState::normal_exit();
                    SuperviseeItem::Exit(SuperviseeExit::NormalExit)
                }
                Err(join_error) => {
                    self.state = SuperviseeState::join_error();
                    SuperviseeItem::Exit(SuperviseeExit::JoinError(join_error))
                }
            },

            SuperviseeState::ShuttingDown { child } => match child.await {
                Ok(()) => {
                    self.state = SuperviseeState::normal_shutdown();
                    SuperviseeItem::Exit(SuperviseeExit::NormalShutdown)
                }
                Err(join_error) => {
                    self.state = SuperviseeState::join_error();
                    SuperviseeItem::Exit(SuperviseeExit::JoinError(join_error))
                }
            },
        }
    }

    pub(super) fn cfg(&self) -> &ChildConfig {
        self.spec.cfg()
    }
}

impl Stream for Supervisee {
    type Item = SuperviseeNext;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let item = pin!(self.as_mut().supervise()).poll(cx);

        item.map(|item| {
            Some(SuperviseeNext {
                pid: self.pid().clone(),
                item,
            })
        })
    }
}

impl ActorOps for Supervisee {
    type Ctx = Dyn;

    fn handle(&self) -> &Channel<Self::Ctx> {
        self.spec.handle()
    }
}

#[derive(Debug)]
pub struct SuperviseeNext {
    pub pid: Pid,
    pub item: SuperviseeItem,
}

#[derive(Debug)]
pub enum SuperviseeItem {
    StartError(StartSuperviseeError),
    Exit(SuperviseeExit),
    Started,
    Initialized,
}

#[derive(Debug)]
pub enum SuperviseeExit {
    JoinError(JoinError),
    NormalExit,
    NormalShutdown,
}

impl SuperviseeExit {
    pub fn should_restart(&self, mode: RestartMode) -> bool {
        match (mode, self) {
            (RestartMode::Always, _) => true,
            (RestartMode::Never, _) => false,
            (RestartMode::OnError, SuperviseeExit::NormalExit | SuperviseeExit::NormalShutdown) => {
                false
            }
            (RestartMode::OnError, SuperviseeExit::JoinError(_)) => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeadSuperviseeStatus {
    JoinError,
    StartError,
    Idle,
    StartCancelled,
    NormalExit,
    NormalShutdown,
}

#[derive(Debug)]
enum SuperviseeState {
    /// The supervisee is starting, and has not yet been spawned.
    Starting { fut: StartFuture },

    /// The supervisee has been spawned, and might be initialized.
    Alive { child: Child },

    /// The supervisee is exiting, and is in the process of shutting down.
    ShuttingDown { child: ExitingChild },

    /// The supervisee has exited, and is no longer running.
    Dead { status: DeadSuperviseeStatus },
}

impl SuperviseeState {
    fn idle() -> Self {
        SuperviseeState::Dead {
            status: DeadSuperviseeStatus::Idle,
        }
    }

    fn start_cancelled() -> Self {
        SuperviseeState::Dead {
            status: DeadSuperviseeStatus::StartCancelled,
        }
    }

    fn start_error() -> Self {
        SuperviseeState::Dead {
            status: DeadSuperviseeStatus::StartError,
        }
    }

    fn join_error() -> Self {
        SuperviseeState::Dead {
            status: DeadSuperviseeStatus::JoinError,
        }
    }

    fn normal_exit() -> Self {
        SuperviseeState::Dead {
            status: DeadSuperviseeStatus::NormalExit,
        }
    }

    fn normal_shutdown() -> Self {
        SuperviseeState::Dead {
            status: DeadSuperviseeStatus::NormalShutdown,
        }
    }

    pub fn status(&self) -> SuperviseeStatus {
        match self {
            SuperviseeState::Starting { .. } => SuperviseeStatus::Starting,
            SuperviseeState::Alive { child } => match child.status() {
                ActorStatus::Exited(_) => SuperviseeStatus::Dead,
                ActorStatus::Initializing => SuperviseeStatus::Initializing,
                ActorStatus::Suspended | ActorStatus::Running => SuperviseeStatus::Initialized,
                ActorStatus::Stopping => SuperviseeStatus::ShuttingDown,
            },
            SuperviseeState::ShuttingDown { .. } => SuperviseeStatus::ShuttingDown,
            SuperviseeState::Dead { .. } => SuperviseeStatus::Dead,
        }
    }

    pub fn map<T>(&mut self, f: impl FnOnce(Self) -> (Self, T)) -> T {
        let old_state = std::mem::replace(self, SuperviseeState::idle());
        let (new_state, result) = f(old_state);
        *self = new_state;
        result
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuperviseeStatus {
    Starting,
    Initializing,
    Initialized,
    ShuttingDown,
    Dead,
}

#[derive(Debug, thiserror::Error)]
pub enum StartSuperviseeError {
    #[error("Concurrent inbox error")]
    ConcurrentInbox,

    #[error("Instantiation error: {0}")]
    Instantiation(Report),

    #[error("Timeout error")]
    Timeout,
}

impl From<StartOnError> for StartSuperviseeError {
    fn from(err: StartOnError) -> Self {
        match err {
            StartOnError::ConcurrentInbox => StartSuperviseeError::ConcurrentInbox,
            StartOnError::Instantiation(e) => StartSuperviseeError::Instantiation(e.into()),
        }
    }
}

impl From<Elapsed> for StartSuperviseeError {
    fn from(_err: Elapsed) -> Self {
        StartSuperviseeError::Timeout
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Cannot start supervisee: {0:?}")]
pub enum SuperviseeStartError {
    #[error("Supervisee is shutting down")]
    IsShuttingDown,
}

struct StartFuture(
    Pin<Box<dyn std::future::Future<Output = Result<Child, StartSuperviseeError>> + Send>>,
);

impl StartFuture {
    fn new(
        fut: impl std::future::Future<Output = Result<Child, StartSuperviseeError>> + Send + 'static,
    ) -> Self {
        StartFuture(Box::pin(fut))
    }
}

impl Future for StartFuture {
    type Output = Result<Child, StartSuperviseeError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.as_mut().poll(cx)
    }
}

impl Debug for StartFuture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StartFuture").finish()
    }
}
