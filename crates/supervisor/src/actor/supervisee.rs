use crate::_prelude::*;
use futures::{
    Stream,
    future::{self, BoxFuture},
};
use rootcause::Report;
use std::{
    fmt::Debug,
    pin::{Pin, pin},
    task::{Context, Poll, Waker},
};
use tokio::time::{error::Elapsed, timeout};
use zestors_runtime::{
    ActorRef, Address, Child, Dyn, ExitStatus, Name, ShutdownChild, errors::JoinError,
};
use zestors_supervision::{ChildConfig, ChildDescription, RestartBackoff, StartOnError};

#[derive(Debug)]
pub(super) struct Supervisee {
    spec: ChildSpec,
    state: SuperviseeState,
    restarter: Option<RestartLimiter>,
    backoff: BackoffState,
    /// The waker from this supervisee's most recent poll. `start`/`stop`
    /// mutate `state` from outside of a poll (e.g. in response to a
    /// sibling's exit), which on its own doesn't get this supervisee's
    /// stream re-polled: `StreamUnordered` only re-enqueues a stream when
    /// something wakes it, and once a `Dead` poll returns `Pending` via
    /// `future::pending()`, nothing ever will on its own. Waking here after
    /// every external mutation is what gets the new state actually observed.
    waker: Option<Waker>,
}

/// How many consecutive restarts a child has needed since it last
/// initialized successfully, used to pick the next [`RestartBackoff`] delay.
#[derive(Debug, Default)]
struct BackoffState {
    consecutive_restarts: u32,
}

impl BackoffState {
    fn next_delay(&mut self, backoff: &RestartBackoff) -> Duration {
        let delay = backoff.delay(self.consecutive_restarts);
        self.consecutive_restarts = self.consecutive_restarts.saturating_add(1);
        delay
    }

    fn reset(&mut self) {
        self.consecutive_restarts = 0;
    }
}

impl Supervisee {
    pub(super) fn new(spec: ChildSpec) -> Self {
        Self {
            restarter: spec.cfg().intensity.clone().map(RestartLimiter::new),
            spec,
            state: SuperviseeState::dead(),
            backoff: BackoffState::default(),
            waker: None,
        }
    }

    fn wake(&self) {
        if let Some(waker) = &self.waker {
            waker.wake_by_ref();
        }
    }

    pub(super) fn get_description(&self) -> ChildDescription {
        ChildDescription {
            name: self.name().clone(),
            cfg: self.spec.cfg().clone(),
        }
    }

    /// Whether this type of exit requires the child to be restarted.
    #[must_use]
    pub(super) fn requires_restart(&self, exit: &SuperviseeExit) -> bool {
        match (self.cfg().restart_mode, exit) {
            (RestartMode::Always, _) => true,
            (RestartMode::Never, _) => false,
            (RestartMode::OnError, SuperviseeExit::NormalExit | SuperviseeExit::NormalShutdown) => {
                false
            }
            (RestartMode::OnError, SuperviseeExit::JoinError(_)) => true,
        }
    }

    /// Whether exiting with this error before finishing initialization
    /// requires the child to be restarted. A normal exit is treated the same
    /// as [`SuperviseeExit::NormalExit`]: under `OnError` it's a successful
    /// completion, not a failure, even though it happened before the child
    /// reported itself as running.
    #[must_use]
    pub(super) fn requires_restart_for_init_exit(&self, error: &InitError) -> bool {
        match (self.cfg().restart_mode, error) {
            (RestartMode::Always, _) => true,
            (RestartMode::Never, _) => false,
            (RestartMode::OnError, InitError::NormalExit) => false,
            (RestartMode::OnError, InitError::Join(_)) => true,
        }
    }

    /// Whether the restart-limiter allows the child to be restarted
    #[must_use]
    pub(super) fn acquire_restart_permit(&mut self) -> bool {
        self.restarter
            .as_mut()
            .map(|r| r.acquire_permit())
            .unwrap_or(true)
    }

    pub(super) fn start(&mut self) -> Result<bool, SuperviseeIsShuttingDown> {
        self.start_inner(None)
    }

    /// Like [`Supervisee::start`], but for a restart: the child is spawned
    /// only after this child's configured [`RestartBackoff`] delay, which
    /// grows with every consecutive failing restart.
    pub(super) fn restart(&mut self) -> Result<bool, SuperviseeIsShuttingDown> {
        self.start_inner(Some(()))
    }

    fn start_inner(&mut self, restart: Option<()>) -> Result<bool, SuperviseeIsShuttingDown> {
        let result = self.state.map(|state| match state {
            // If the supervisee is idle or dead, we can start it.
            SuperviseeState::Dead { .. } => {
                let delay = match restart {
                    Some(()) => self.backoff.next_delay(&self.spec.cfg().backoff),
                    None => Duration::ZERO,
                };
                let spec = self.spec.clone();
                let fut = StartFuture::new(async move {
                    if !delay.is_zero() {
                        tokio::time::sleep(delay).await;
                    }
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
        });

        // This state change happens outside of a poll, so nothing re-polls
        // this supervisee's stream on its own; wake it explicitly (see
        // `waker`'s doc comment).
        self.wake();

        result
    }

    pub(super) fn stop(&mut self) -> StopOutcome {
        let outcome = self.state.map(|state| match state {
            // If the supervisee is dead or idle, there's nothing to do.
            SuperviseeState::Dead { .. } => (state, StopOutcome::Dead),

            // If the supervisee is starting, we drop the future and mark it as
            // dead. This resolves synchronously: no exit event will follow.
            SuperviseeState::Starting { fut } => {
                drop(fut);
                (SuperviseeState::dead(), StopOutcome::Cancelled)
            }

            // If the supervisee is still initializing, the init-monitor future
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
        });

        // See `waker`'s doc comment: this state change happens outside of a
        // poll, so it needs an explicit wake to actually be observed.
        self.wake();

        outcome
    }

    pub(super) async fn supervise(&mut self) -> SuperviseeItem {
        let item = match &mut self.state {
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
                    self.state = SuperviseeState::dead();
                    SuperviseeItem::Started(Err(e))
                }
            },

            SuperviseeState::Initializing { fut, .. } => match fut.await {
                Ok(()) => {
                    let SuperviseeState::Initializing { child, .. } =
                        std::mem::replace(&mut self.state, SuperviseeState::dead())
                    else {
                        unreachable!();
                    };

                    self.state = SuperviseeState::Alive { child };
                    SuperviseeItem::Initialized(Ok(()))
                }
                Err(_status) => {
                    let SuperviseeState::Initializing { child, .. } =
                        std::mem::replace(&mut self.state, SuperviseeState::dead())
                    else {
                        unreachable!();
                    };

                    // The channel only tracks *that* the child failed, not the
                    // error it returned. The task has already finished (that's
                    // what `monitor_init` observed), so joining it here is
                    // immediate, and gives us the child's own `Report` to log
                    // and to attach to the supervisor's eventual failure.
                    let error = match child.await {
                        Ok(()) => InitError::NormalExit,
                        Err(join_error) => InitError::Join(join_error),
                    };
                    SuperviseeItem::Initialized(Err(error))
                }
            },

            SuperviseeState::Alive { child } => match child.await {
                Ok(()) => {
                    self.state = SuperviseeState::dead();
                    SuperviseeItem::Exit(SuperviseeExit::NormalExit)
                }
                Err(join_error) => {
                    self.state = SuperviseeState::dead();
                    SuperviseeItem::Exit(SuperviseeExit::JoinError(join_error))
                }
            },

            SuperviseeState::ShuttingDown { child } => match child.await {
                Ok(()) => {
                    self.state = SuperviseeState::dead();
                    SuperviseeItem::Exit(SuperviseeExit::NormalShutdown)
                }
                Err(join_error) => {
                    self.state = SuperviseeState::dead();
                    SuperviseeItem::Exit(SuperviseeExit::JoinError(join_error))
                }
            },
        };

        // A child that made it back to `Running` after this restart is healthy
        // again, so the next restart starts from the beginning of the backoff.
        if matches!(&item, SuperviseeItem::Initialized(Ok(()))) {
            self.backoff.reset();
        }

        item
    }

    pub(super) fn cfg(&self) -> &ChildConfig {
        self.spec.cfg()
    }
}

impl Stream for Supervisee {
    type Item = SuperviseeNext;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.waker = Some(cx.waker().clone());

        let item = pin!(self.as_mut().supervise()).poll(cx);

        item.map(|item| {
            Some(SuperviseeNext {
                name: self.name().clone(),
                item,
            })
        })
    }
}

impl ActorRef for Supervisee {
    type Ctx = Dyn;

    fn actor_ref(&self) -> &Address<Self::Ctx> {
        self.spec.actor_ref()
    }
}

#[derive(Debug)]
pub(super) struct SuperviseeNext {
    pub(super) name: Name,
    pub(super) item: SuperviseeItem,
}

#[derive(Debug)]
pub(super) enum SuperviseeItem {
    Started(Result<(), StartSuperviseeError>),
    Initialized(Result<(), InitError>),
    Exit(SuperviseeExit),
}

/// Why a child that was spawned did not reach `Running`.
#[derive(Debug, thiserror::Error)]
pub(super) enum InitError {
    /// The child's task ended with an error, panicked, or was aborted.
    #[error("{0}")]
    Join(#[from] JoinError),

    /// The child's task returned `Ok(())` without ever reporting itself as
    /// initialized.
    #[error("child exited normally before finishing initialization")]
    NormalExit,
}

#[derive(Debug)]
pub(super) enum SuperviseeExit {
    JoinError(JoinError),
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
        child: ShutdownChild,
    },

    /// The supervisee has exited, and is no longer running.
    Dead,
}

impl SuperviseeState {
    fn dead() -> Self {
        SuperviseeState::Dead
    }

    fn map<T>(&mut self, f: impl FnOnce(Self) -> (Self, T)) -> T {
        let old_state = std::mem::replace(self, SuperviseeState::dead());
        let (new_state, result) = f(old_state);
        *self = new_state;
        result
    }
}

/// The result of calling [`Supervisee::stop`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StopOutcome {
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
    pub(super) fn is_shutting_down(self) -> bool {
        matches!(self, StopOutcome::ShuttingDown)
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum StartSuperviseeError {
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
            StartOnError::Instantiation(e) => StartSuperviseeError::Instantiation(e),
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
pub(super) struct SuperviseeIsShuttingDown;

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
        InitFuture(Box::pin(async move { address.monitor_init().await }))
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
