use super::*;
use futures::{FutureExt, Stream, StreamExt as _};
use std::{
    pin::Pin,
    task::{Context, Poll, ready},
};

pub(super) struct Supervisee<S: Start = DynStarter> {
    spec: ChildSpec<S>,
    dynamic: bool,
    state: SuperviseeState<S>,
}

pub(super) enum SuperviseeState<S: Start = DynStarter> {
    Starting {
        fut: Pin<
            Box<dyn Future<Output = Result<Child<S::Exit, S::Ctx>, StartSuperviseeError>> + Send>,
        >,
    },

    Alive {
        child: Child<S::Exit, S::Ctx>,
    },

    Exiting {
        child: ExitingChild<S::Exit, S::Ctx>,
    },

    Dead {},
}

impl<S: Start> Supervisee<S> {
    pub(super) fn new_static(spec: ChildSpec<S>) -> Self {
        Self {
            state: SuperviseeState::Dead {},
            spec,
            dynamic: false,
        }
    }

    pub(super) fn new_dynamic(spec: ChildSpec<S>) -> Self {
        Self {
            state: SuperviseeState::Dead {},
            spec,
            dynamic: true,
        }
    }

    pub(super) fn spec(&self) -> &ChildSpec<S> {
        &self.spec
    }

    pub(super) fn cfg(&self) -> &ChildConfig {
        self.spec.cfg()
    }

    pub(super) fn child_mut(&mut self) -> Option<&mut Child<S::Exit, S::Ctx>> {
        match &mut self.state {
            SuperviseeState::Alive { child, .. } => Some(child),
            _ => None,
        }
    }

    pub(super) fn into_child(self) -> Option<Child<S::Exit, S::Ctx>> {
        match self.state {
            SuperviseeState::Alive { child, .. } => Some(child),
            SuperviseeState::Exiting { child } => Some(child.into_inner()),
            _ => None,
        }
    }

    pub(super) fn exiting_child_mut(&mut self) -> Option<&mut ExitingChild<S::Exit, S::Ctx>> {
        match &mut self.state {
            SuperviseeState::Exiting { child, .. } => Some(child),
            _ => None,
        }
    }

    pub(super) fn is_dynamic(&self) -> bool {
        self.dynamic
    }

    pub(super) fn start(&mut self) -> bool
    where
        S: Clone + Send + Sync + 'static,
    {
        match &mut self.state {
            SuperviseeState::Dead {} => {
                self.state = SuperviseeState::Starting {
                    fut: self.start_fut(),
                };

                true
            }
            _ => false,
        }
    }

    pub(super) fn initiate_shutdown(&mut self) -> Option<&mut ExitingChild<S::Exit, S::Ctx>> {
        let abort_timeout = self.spec.cfg().abort_timeout;

        let res = self.map_state(|state| match state {
            SuperviseeState::Alive { child } => {
                let fut = child.into_shutdown(abort_timeout);
                (SuperviseeState::Exiting { child: fut }, true)
            }

            state => (state, false),
        });

        match res {
            true => Some(self.exiting_child_mut().unwrap()),
            false => None,
        }
    }

    fn map_state<T>(
        &mut self,
        fun: impl FnOnce(SuperviseeState<S>) -> (SuperviseeState<S>, T),
    ) -> T {
        let state = std::mem::replace(&mut self.state, SuperviseeState::Dead {});
        let (state, res) = fun(state);
        self.state = state;
        res
    }

    fn start_fut(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Child<S::Exit, S::Ctx>, StartSuperviseeError>> + Send>>
    where
        S: Clone + Send + Sync + 'static,
    {
        let spec = self.spec.clone();
        let duration = spec.cfg().start_timeout;

        Box::pin(async move {
            tokio::time::timeout(duration, spec.start())
                .await
                .map_err(|_| StartSuperviseeError::Timeout)?
                .map_err(|e| e.into())
        })
    }
}

impl ActorOps for Supervisee {
    type Ctx = Set<()>;

    fn handle(&self) -> &Channel<Self::Ctx> {
        &self.spec.handle()
    }
}

impl<S: Start> Debug for Supervisee<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Supervisee")
            .field("pid", &self.spec.pid())
            .field("dynamic", &self.dynamic)
            .finish()
    }
}

impl<S: Start + Clone + Send + Sync + Unpin + 'static> Stream for Supervisee<S> {
    type Item = SuperviseeEvent<S>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match &mut self.state {
                SuperviseeState::Starting { fut } => {
                    let start_result = ready!(fut.poll_unpin(cx));

                    match start_result {
                        Ok(child) => {
                            self.state = SuperviseeState::Alive { child };
                        }
                        Err(e) => {
                            self.state = SuperviseeState::Dead {};
                            return Poll::Ready(Some(SuperviseeEvent::StartFailed(e.into())));
                        }
                    }
                }

                SuperviseeState::Alive { child } => {
                    let exit_result = ready!(child.poll_unpin(cx));
                    self.state = SuperviseeState::Dead {};
                    return Poll::Ready(Some(SuperviseeEvent::Exited(exit_result)));
                }

                SuperviseeState::Exiting { child } => {
                    let exit_result = ready!(child.poll_unpin(cx));
                    self.state = SuperviseeState::Dead {};
                    return Poll::Ready(Some(SuperviseeEvent::Exited(exit_result)));
                }

                SuperviseeState::Dead {} => {
                    return Poll::Pending;
                }
            }
        }
    }
}

pub(super) enum SuperviseeEvent<S: Start = DynStarter> {
    Exited(Result<S::Exit, JoinError>),
    StartFailed(StartSuperviseeError),
}

impl<S: Start> SuperviseeEvent<S> {
    pub(super) fn into_result(self) -> Result<S::Exit, Report> {
        match self {
            SuperviseeEvent::Exited(exit_result) => exit_result.map_err(|e| e.into()),
            SuperviseeEvent::StartFailed(e) => Err(Report::from(e)),
        }
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
    fn from(e: StartOnError) -> Self {
        match e {
            StartOnError::ConcurrentInbox => StartSuperviseeError::ConcurrentInbox,
            StartOnError::Instantiation(e) => StartSuperviseeError::Instantiation(e.into()),
        }
    }
}
