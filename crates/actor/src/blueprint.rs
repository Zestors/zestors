use crate::{Actor, ActorExt as _, RestartIntensity, RestartMode};
use std::{fmt::Debug, future::Future, time::Duration};
use zestors_runtime::{errors::DuplicatePidError, prelude::*};

pub trait ActorBlueprint: Debug + Send + Sync + 'static {
    type Actor: Actor;

    fn instantiate(&self) -> impl Future<Output = rootcause::Result<Self::Actor>> + Send;

    fn default_instantiation_timeout(&self) -> Duration {
        Duration::from_millis(5_000)
    }

    fn default_abort_timeout(&self) -> Duration {
        Duration::from_millis(5_000)
    }

    fn default_init_timeout(&self) -> Duration {
        Duration::from_millis(5_000)
    }

    fn default_restart_mode(&self) -> RestartMode {
        RestartMode::default()
    }

    fn default_restart_intensity(&self) -> Option<RestartIntensity> {
        None
    }
}

impl<T: Actor + Clone + Debug + Send + Sync + 'static> ActorBlueprint for T {
    type Actor = T;

    async fn instantiate(&self) -> rootcause::Result<Self::Actor> {
        Ok(self.clone())
    }
}

pub trait BlueprintExt: ActorBlueprint + Sized {
    fn start_with(
        &self,
        pid: Pid,
    ) -> impl Future<
        Output = Result<
            Child<<Self::Actor as Actor>::Exit, <Self::Actor as Actor>::Interface>,
            InstantiateWithError,
        >,
    > + Send
    where
        Self: Send + Sync + 'static,
    {
        async {
            self.instantiate()
                .await
                .map_err(InstantiateWithError::InstantiationFailed)?
                .spawn_with(pid)
                .map_err(Into::into)
        }
    }
}
impl<T: ActorBlueprint> BlueprintExt for T {}

pub struct FnBlueprint<F, A>
where
    F: Fn() -> A + Send + Sync + 'static,
    A: Actor,
{
    f: F,
    restart_mode: RestartMode,
}

impl<F, A> FnBlueprint<F, A>
where
    F: Fn() -> A + Send + Sync + 'static,
    A: Actor,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            restart_mode: RestartMode::default(),
        }
    }

    pub fn with_restart_mode(mut self, restart_mode: RestartMode) -> Self {
        self.restart_mode = restart_mode;
        self
    }
}

impl<F, A> ActorBlueprint for FnBlueprint<F, A>
where
    F: Fn() -> A + Send + Sync + 'static,
    A: Actor,
{
    type Actor = A;

    async fn instantiate(&self) -> rootcause::Result<Self::Actor> {
        Ok((self.f)())
    }

    fn default_restart_mode(&self) -> RestartMode {
        self.restart_mode
    }
}

impl<F, A> Debug for FnBlueprint<F, A>
where
    F: Fn() -> A + Send + Sync + 'static,
    A: Actor,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FnBlueprint")
            .field("actor", &std::any::type_name::<A>())
            .finish()
    }
}

impl<F, A> Clone for FnBlueprint<F, A>
where
    F: Clone + Fn() -> A + Send + Sync + 'static,
    A: Actor,
{
    fn clone(&self) -> Self {
        Self {
            f: self.f.clone(),
            restart_mode: self.restart_mode,
        }
    }
}

pub fn blueprint_fn<F, A>(f: F) -> FnBlueprint<F, A>
where
    F: Fn() -> A + Send + Sync + 'static,
    A: Actor,
{
    FnBlueprint::new(f)
}

#[derive(Debug, thiserror::Error)]
pub enum InstantiateWithError {
    #[error("Actor-instantiation failed: {0}")]
    InstantiationFailed(rootcause::Report),

    #[error("Spawning failed: {0}")]
    DuplicatePid(
        #[from]
        #[source]
        DuplicatePidError,
    ),
}
