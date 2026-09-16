use crate::_prelude::*;
use serde::{Deserialize, Serialize};
use zestors_runtime::{
    ActorRef, Channel,
    errors::{DuplicatePidError, StartOnError},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildConfig {
    pub restart_mode: RestartMode,
    pub intensity: RestartIntensity,
    pub abort_timeout: Duration,
    pub init_timeout: Duration,
    pub start_timeout: Duration,
}

impl Default for ChildConfig {
    fn default() -> Self {
        Self {
            restart_mode: RestartMode::Always,
            intensity: RestartIntensity::default(),
            abort_timeout: Duration::from_secs(5),
            init_timeout: Duration::from_secs(5),
            start_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChildDescription {
    pub pid: Pid,
    pub cfg: ChildConfig,
}

impl ChildConfig {
    pub fn from_blueprint<T: Blueprint>(blueprint: &T) -> Self {
        Self {
            restart_mode: blueprint.default_restart_mode(),
            abort_timeout: blueprint.default_abort_timeout(),
            init_timeout: blueprint.default_init_timeout(),
            start_timeout: blueprint.default_instantiation_timeout(),
            intensity: blueprint.default_restart_intensity(),
        }
    }
}

pub struct ChildSpec<T: Start = DynStarter> {
    cfg: ChildConfig,
    blueprint: T,
    channel: StrongAddress<T::Ctx>,
}

// Implementations just when T is statically known
impl<T: Blueprint> ChildSpec<T> {
    pub fn create(id: impl Into<Pid>, blueprint: T) -> Result<Self, DuplicatePidError> {
        Ok(Self {
            channel: StrongAddress::<<T::Actor as Actor>::Interface>::create(id.into())?,
            cfg: ChildConfig::from_blueprint(&blueprint),
            blueprint,
        })
    }

    pub fn create_rand_pid(blueprint: T) -> Self {
        Self::create(Pid::rand(), blueprint).expect("Pid is unique")
    }

    pub fn split(self) -> (ChildSpec, Address<<T::Actor as Actor>::Interface>) {
        let address = self.channel.address().clone();
        (self.into_dyn(), address)
    }
}

// Implementations when T can be any type that implements RepeatSpawn
// (including DynRepeatSpawner)
impl<T: Start> ChildSpec<T> {
    pub fn cfg(&self) -> &ChildConfig {
        &self.cfg
    }

    pub fn blueprint(&self) -> &T {
        &self.blueprint
    }

    pub fn blueprint_mut(&mut self) -> &mut T {
        &mut self.blueprint
    }

    pub fn with_mode(mut self, restart_mode: RestartMode) -> Self {
        self.cfg.restart_mode = restart_mode;
        self
    }

    pub fn with_abort_timeout(mut self, abort_timeout: Duration) -> Self {
        self.cfg.abort_timeout = abort_timeout;
        self
    }

    pub fn with_init_timeout(mut self, init_timeout: Duration) -> Self {
        self.cfg.init_timeout = init_timeout;
        self
    }

    pub fn with_cfg(mut self, cfg: ChildConfig) -> Self {
        self.cfg = cfg;
        self
    }

    pub async fn start(&self) -> Result<Child<T::Exit, T::Ctx>, StartOnError> {
        self.blueprint.start_on(self.channel.clone()).await
    }

    pub fn into_dyn(self) -> ChildSpec {
        ChildSpec {
            cfg: self.cfg,
            blueprint: self.blueprint.into(),
            channel: self.channel.into_dyn(),
        }
    }
}

impl<T: Start> ActorRef for ChildSpec<T> {
    type Ctx = T::Ctx;

    fn channel(&self) -> &Channel<Self::Ctx> {
        &self.channel.channel()
    }
}

impl<T: Start + Clone> Clone for ChildSpec<T> {
    fn clone(&self) -> Self {
        Self {
            cfg: self.cfg.clone(),
            blueprint: self.blueprint.clone(),
            channel: self.channel.clone(),
        }
    }
}

impl<T: Start + Debug> Debug for ChildSpec<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChildSpec")
            .field("cfg", &self.cfg)
            .field("spawner", &self.blueprint)
            .field("data", &self.channel)
            .finish()
    }
}

impl<T: Blueprint> From<ChildSpec<T>> for ChildSpec {
    fn from(spec: ChildSpec<T>) -> Self {
        spec.into_dyn()
    }
}

pub trait BlueprintSupervisionExt: Blueprint + Sized {
    fn into_spawn_fn(self) -> DynStarter
    where
        Self: Send + Sync + 'static,
    {
        DynStarter::new(self)
    }

    fn pid(self, pid: impl Into<Pid>) -> Result<ChildSpec<Self>, DuplicatePidError> {
        ChildSpec::create(pid, self)
    }

    fn with_rand_pid(self) -> ChildSpec<Self> {
        ChildSpec::create_rand_pid(self)
    }
}
impl<T: Blueprint> BlueprintSupervisionExt for T {}
