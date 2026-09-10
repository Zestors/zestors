use crate::_prelude::*;
use std::{future::Future, sync::Mutex};

pub trait Blueprint: Debug + Send + Sync + 'static {
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
}

impl<T: Actor + Clone + Debug + Send + Sync + 'static> Blueprint for T {
    type Actor = T;

    async fn instantiate(&self) -> rootcause::Result<Self::Actor> {
        Ok(self.clone())
    }
}

pub trait BlueprintExt: Blueprint + Sized {
    fn into_spawn_fn(self) -> DynStarter
    where
        Self: Send + Sync + 'static,
    {
        DynStarter::new(self)
    }

    fn start_with(
        &self,
        pid: Pid,
    ) -> impl Future<
        Output = Result<
            Child<<Self::Actor as Actor>::Exit, <Self::Actor as Actor>::Interface>,
            StartWithError,
        >,
    > + Send
    where
        Self: Send + Sync + 'static,
    {
        async {
            self.instantiate()
                .await
                .map_err(StartWithError::InstantiationFailed)?
                .spawn_with(pid)
                .map_err(Into::into)
        }
    }

    fn generate_config(&self) -> ChildConfig {
        ChildConfig::new_for_blueprint(self)
    }

    fn with_pid(self, pid: impl Into<Pid>) -> Result<ChildSpec<Self>, DuplicatePidError> {
        ChildSpec::create(pid, self)
    }

    fn with_rand_pid(self) -> ChildSpec<Self> {
        ChildSpec::create_rand_pid(self)
    }
}
impl<T: Blueprint> BlueprintExt for T {}

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

impl<F, A> Blueprint for FnBlueprint<F, A>
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
pub enum StartWithError {
    #[error("Instantiation failed: {0}")]
    InstantiationFailed(rootcause::Report),

    #[error("Spawn failed: {0}")]
    DuplicatePid(
        #[from]
        #[source]
        DuplicatePidError,
    ),
}
