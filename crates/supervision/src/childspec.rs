use crate::_prelude::*;
use serde::{Deserialize, Serialize};
use zestors_runtime::{
    ActorRef, Address,
    errors::{DuplicatePidError, StartOnError},
};

/// The settings a `Supervisor` applies to one
/// of its children: when to
/// restart it, how many restarts to allow, and how long to wait for it to
/// start, initialize, and shut down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildConfig {
    /// Whether the child should be restarted after it exits.
    pub restart_mode: RestartMode,

    /// A restart-rate limit specific to this child, in addition to the
    /// supervisor's own. `None` means this child is only bound by the
    /// supervisor-wide limit.
    pub intensity: Option<RestartIntensity>,

    /// How long to wait for the child's task to exit after it is aborted.
    pub abort_timeout: Duration,

    /// How long to wait for the child to finish initializing.
    pub init_timeout: Duration,

    /// How long to wait for [`ActorBlueprint::instantiate`] to complete when
    /// (re)starting the child.
    pub start_timeout: Duration,
}

impl Default for ChildConfig {
    fn default() -> Self {
        Self {
            restart_mode: RestartMode::Always,
            intensity: Default::default(),
            abort_timeout: Duration::from_secs(5),
            init_timeout: Duration::from_secs(5),
            start_timeout: Duration::from_secs(5),
        }
    }
}

/// A snapshot of a child's identity and configuration, as returned by
/// [`GetChildren`](crate::messages::GetChildren) and used to rebuild a
/// [`SupervisionTree`](crate::SupervisionTree).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChildDescription {
    /// The child's [`Pid`].
    pub pid: Pid,
    /// The child's [`ChildConfig`].
    pub cfg: ChildConfig,
}

impl ChildConfig {
    /// Builds a [`ChildConfig`] from a blueprint's `default_*` methods,
    /// leaving [`ChildConfig::intensity`] unset.
    pub fn from_blueprint<T: ActorBlueprint>(blueprint: &T) -> Self {
        Self {
            restart_mode: blueprint.default_restart_mode(),
            abort_timeout: blueprint.default_abort_timeout(),
            init_timeout: blueprint.default_init_timeout(),
            start_timeout: blueprint.default_instantiation_timeout(),
            intensity: None,
        }
    }
}

/// A registered [`Pid`] together with a blueprint and the [`ChildConfig`] a
/// `Supervisor` should apply to it —
/// everything needed to (re)start the
/// child on demand.
///
/// `T` is the blueprint type; it defaults to [`DynStarter`], the type-erased
/// form used once a spec is handed to a
/// `Supervisor` (see
/// [`ChildSpec::into_dyn`]).
pub struct ChildSpec<T: Start = DynStarter> {
    cfg: ChildConfig,
    blueprint: T,
    channel: StrongAddress<T::Ctx>,
}

// Implementations just when T is statically known
impl<T: ActorBlueprint> ChildSpec<T> {
    /// Creates a spec for `blueprint` registered under `id`, with
    /// [`ChildConfig`] defaults taken from the blueprint. Fails if `id` is
    /// already registered.
    pub fn create(id: impl Into<Pid>, blueprint: T) -> Result<Self, DuplicatePidError> {
        Ok(Self {
            channel: StrongAddress::<<T::Actor as Actor>::Interface>::create(id.into())?,
            cfg: ChildConfig::from_blueprint(&blueprint),
            blueprint,
        })
    }

    /// Same as [`ChildSpec::create`], with a freshly generated [`Pid`].
    pub fn create_rand_pid(blueprint: T) -> Self {
        Self::create(Pid::rand(), blueprint).expect("Pid is unique")
    }

    /// Splits off the child's [`Address`], returning it alongside the
    /// type-erased spec.
    pub fn split(self) -> (ChildSpec, Address<<T::Actor as Actor>::Interface>) {
        let address = self.channel.address().clone();
        (self.into_dyn(), address)
    }
}

// Implementations when T can be any type that implements RepeatSpawn
// (including DynRepeatSpawner)
impl<T: Start> ChildSpec<T> {
    /// Returns this spec's [`ChildConfig`].
    pub fn cfg(&self) -> &ChildConfig {
        &self.cfg
    }

    /// Returns this spec's blueprint.
    pub fn blueprint(&self) -> &T {
        &self.blueprint
    }

    /// Returns a mutable reference to this spec's blueprint.
    pub fn blueprint_mut(&mut self) -> &mut T {
        &mut self.blueprint
    }

    /// Sets [`ChildConfig::restart_mode`].
    pub fn with_mode(mut self, restart_mode: RestartMode) -> Self {
        self.cfg.restart_mode = restart_mode;
        self
    }

    /// Sets [`ChildConfig::abort_timeout`].
    pub fn with_abort_timeout(mut self, abort_timeout: Duration) -> Self {
        self.cfg.abort_timeout = abort_timeout;
        self
    }

    /// Sets [`ChildConfig::init_timeout`].
    pub fn with_init_timeout(mut self, init_timeout: Duration) -> Self {
        self.cfg.init_timeout = init_timeout;
        self
    }

    /// Replaces this spec's whole [`ChildConfig`].
    pub fn with_cfg(mut self, cfg: ChildConfig) -> Self {
        self.cfg = cfg;
        self
    }

    /// Instantiates and spawns the child, returning its [`Child`] handle.
    pub async fn start(&self) -> Result<Child<T::Exit, T::Ctx>, StartOnError> {
        self.blueprint.start_on(self.channel.clone()).await
    }

    /// Type-erases this spec's blueprint, for handing it to a
    /// `Supervisor`.
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

    fn actor_ref(&self) -> &Address<Self::Ctx> {
        self.channel.actor_ref()
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

impl<T: ActorBlueprint> From<ChildSpec<T>> for ChildSpec {
    fn from(spec: ChildSpec<T>) -> Self {
        spec.into_dyn()
    }
}

/// Convenience methods for turning an [`ActorBlueprint`] into a [`ChildSpec`],
/// implemented automatically for every [`ActorBlueprint`].
pub trait BlueprintSupervisionExt: ActorBlueprint + Sized {
    /// Type-erases this blueprint into a [`DynStarter`].
    fn into_spawn_fn(self) -> DynStarter
    where
        Self: Send + Sync + 'static,
    {
        DynStarter::new(self)
    }

    /// Creates a [`ChildSpec`] for this blueprint registered under `pid`.
    /// Fails if `pid` is already registered.
    fn pid(self, pid: impl Into<Pid>) -> Result<ChildSpec<Self>, DuplicatePidError> {
        ChildSpec::create(pid, self)
    }

    /// Creates a [`ChildSpec`] for this blueprint under a freshly generated [`Pid`].
    fn with_rand_pid(self) -> ChildSpec<Self> {
        ChildSpec::create_rand_pid(self)
    }
}
impl<T: ActorBlueprint> BlueprintSupervisionExt for T {}
