use crate::_prelude::*;
use crate::ChildSpec;
use futures::{
    Stream,
    future::{self, BoxFuture},
};
use rootcause::Report;
use std::{
    fmt::Debug,
    pin::{Pin, pin},
    task::{Context, Poll},
};
use tokio::time::{error::Elapsed, timeout};
use zestors_runtime::{
    ActorRef, Channel, Child, Dyn, ExitStatus, ExitingChild, Pid,
    errors::{JoinError, StartOnError},
};

#[derive(Debug)]
pub(crate) struct Supervisee {
    spec: ChildSpec,
    state: SuperviseeState,
    restarter: RestartLimiter,
}

impl Supervisee {
    pub(crate) fn new(spec: ChildSpec) -> Self {
        Self {
            restarter: RestartLimiter::new(spec.cfg().intensity.clone()),
            spec,
            state: SuperviseeState::idle(),
        }
    }

    pub(crate) fn get_description(&self) -> ChildDescription {
        ChildDescription {
            pid: self.pid().clone(),
            cfg: self.spec.cfg().clone(),
        }
    }

    /// Whether this type of exit requires the child to be restarted.
    #[must_use]
    pub(crate) fn requires_restart(&self, exit: &SuperviseeExit) -> bool {
        match (self.cfg().restart_mode, exit) {
            (RestartMode::Always, _) => true,
            (RestartMode::Never, _) => false,
            (RestartMode::OnError, SuperviseeExit::NormalExit | SuperviseeExit::NormalShutdown) => {
                false
            }
            (RestartMode::OnError, SuperviseeExit::JoinError(_)) => true,
        }
    }

    /// Whether the restart-limiter allows the child to be restarted
    #[must_use]
    pub(crate) fn acquire_restart_permit(&mut self) -> bool {
        self.restarter.acquire_permit()
    }

    pub(crate) fn start(&mut self) -> Result<bool, SuperviseeIsShuttingDown> {
        self.state.map(|state| match state {
            // If the supervisee is idle or dead, we can start it.
            SuperviseeState::Dead { .. } => {
                let spec = self.spec.clone();
                let fut = StartFuture::new(async move {
                    Ok(timeout(spec.cfg().start_timeout, spec.start()).await??)
                });

                (SuperviseeState::Starting { fut }, Ok(true))
            }

            // If the supervisee is starting, initializing, or alive, all is good
            SuperviseeState::Starting { .. }
            | SuperviseeState::Initializing { .. }
            | SuperviseeState::Alive { .. } => (state, Ok(false)),

            // If the supervisee is shutting down, we cannot start it.
            SuperviseeState::ShuttingDown { .. } => (state, Err(SuperviseeIsShuttingDown)),
        })
    }

    pub(crate) fn stop(&mut self) -> StopOutcome {
        self.state.map(|state| match state {
            // If the supervisee is dead or idle, there's nothing to do.
            SuperviseeState::Dead { .. } => (state, StopOutcome::Dead),

            // If the supervisee is starting, we drop the future and mark it as
            // dead. This resolves synchronously: no exit event will follow.
            SuperviseeState::Starting { fut } => {
                drop(fut);
                (SuperviseeState::start_cancelled(), StopOutcome::Cancelled)
            }

            // If the supervisee is still initializing, the init-watch future
            // no longer matters; initiate a shutdown same as if it were alive.
            SuperviseeState::Initializing { child, .. } | SuperviseeState::Alive { child } => {
                let child = child.into_shutdown(self.spec.cfg().abort_timeout);
                (
                    SuperviseeState::ShuttingDown { child },
                    StopOutcome::ShuttingDown,
                )
            }

            // Already shutting down; nothing new to do, but an exit event is
            // still on its way.
            SuperviseeState::ShuttingDown { child } => (
                SuperviseeState::ShuttingDown { child },
                StopOutcome::ShuttingDown,
            ),
        })
    }

    pub(crate) async fn supervise(&mut self) -> SuperviseeItem {
        match &mut self.state {
            SuperviseeState::Dead { .. } => future::pending().await,

            SuperviseeState::Starting { fut } => match fut.await {
                Ok(child) => {
                    self.state = SuperviseeState::Initializing {
                        fut: InitFuture::new(&child),
                        child,
                    };
                    SuperviseeItem::Started(Ok(()))
                }

                Err(e) => {
                    self.state = SuperviseeState::start_error();
                    SuperviseeItem::Started(Err(e))
                }
            },

            SuperviseeState::Initializing { fut, .. } => match fut.await {
                Ok(()) => {
                    let SuperviseeState::Initializing { child, .. } =
                        std::mem::replace(&mut self.state, SuperviseeState::idle())
                    else {
                        unreachable!();
                    };

                    self.state = SuperviseeState::Alive { child };
                    SuperviseeItem::Initialized(Ok(()))
                }
                Err(e) => {
                    self.state = SuperviseeState::start_error();
                    SuperviseeItem::Initialized(Err(e))
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

impl ActorRef for Supervisee {
    type Ctx = Dyn;

    fn channel(&self) -> &Channel<Self::Ctx> {
        self.spec.channel()
    }
}

#[derive(Debug)]
pub(crate) struct SuperviseeNext {
    pub(crate) pid: Pid,
    pub(crate) item: SuperviseeItem,
}

#[derive(Debug)]
pub(crate) enum SuperviseeItem {
    Started(Result<(), StartSuperviseeError>),
    Initialized(Result<(), ExitStatus>),
    Exit(SuperviseeExit),
}

#[derive(Debug)]
pub(crate) enum SuperviseeExit {
    JoinError(JoinError),
    NormalExit,
    NormalShutdown,
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
    Starting {
        fut: StartFuture,
    },

    Initializing {
        fut: InitFuture,
        child: Child,
    },

    /// The supervisee has been spawned, and might be initialized.
    Alive {
        child: Child,
    },

    /// The supervisee is exiting, and is in the process of shutting down.
    ShuttingDown {
        child: ExitingChild,
    },

    /// The supervisee has exited, and is no longer running.
    Dead {
        status: DeadSuperviseeStatus,
    },
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

    fn map<T>(&mut self, f: impl FnOnce(Self) -> (Self, T)) -> T {
        let old_state = std::mem::replace(self, SuperviseeState::idle());
        let (new_state, result) = f(old_state);
        *self = new_state;
        result
    }
}

/// The result of calling [`Supervisee::stop`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopOutcome {
    /// Was already dead (or idle); nothing happened, and there's no exit
    /// event to wait for.
    Dead,
    /// Was still starting; the in-flight start was cancelled synchronously,
    /// landing straight on `Dead`. There's no exit event to wait for either,
    /// since nothing was ever spawned.
    Cancelled,
    /// Is now (or was already) shutting down; an exit event will eventually
    /// follow.
    ShuttingDown,
}

impl StopOutcome {
    /// Whether an exit event should still be waited for.
    pub(crate) fn is_shutting_down(self) -> bool {
        matches!(self, StopOutcome::ShuttingDown)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum StartSuperviseeError {
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
#[error("Cannot start supervisee: Currently shutting down")]
pub(crate) struct SuperviseeIsShuttingDown;

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

struct InitFuture(BoxFuture<'static, Result<(), ExitStatus>>);

impl InitFuture {
    fn new(child: &Child) -> Self {
        let address = child.address().clone();
        InitFuture(Box::pin(
            async move { address.watch_initialization().await },
        ))
    }
}

impl Future for InitFuture {
    type Output = Result<(), ExitStatus>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.as_mut().poll(cx)
    }
}

impl Debug for InitFuture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InitFuture").finish()
    }
}
