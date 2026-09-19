use crate::{_prelude::*, actor::SuperviseeMap};
use indexmap::IndexMap;
use std::sync::Arc;
use zestors_supervision::{RestartIntensity, Start};

/// A declarative, reusable recipe for a [`Supervisor`]: its child
/// [`ChildSpec`]s, its [`SupervisionStrategy`], an optional restart intensity,
/// and an optional dynamic [`SupervisorSource`].
///
/// Build one with [`SupervisorBlueprint::new`] (or the [`Supervisor::blueprint`]
/// shorthand) and the `child`/`children`/`strategy`/`intensity`/`source`
/// builder methods, then spawn it like any other blueprint.
pub struct SupervisorBlueprint {
    supervisees: IndexMap<Name, ChildSpec>,
    strategy: SupervisionStrategy,
    restart_intensity: Option<RestartIntensity>,
    source: Option<Arc<dyn SupervisorSource>>,
}

impl SupervisorBlueprint {
    /// Creates an empty blueprint: [`SupervisionStrategy::OneForOne`], no
    /// children, no source, and no explicit restart intensity (a
    /// strategy-dependent fallback is applied at instantiation — see
    /// [`SupervisorBlueprint::intensity`]).
    pub fn new() -> Self {
        Self {
            supervisees: Default::default(),
            strategy: SupervisionStrategy::default(),
            restart_intensity: None,
            source: None,
        }
    }

    /// Same as [`SupervisorBlueprint::new`] with an explicit
    /// [`SupervisionStrategy::OneForOne`].
    pub fn one_for_one() -> Self {
        Self::new().strategy(SupervisionStrategy::OneForOne)
    }

    /// Same as [`SupervisorBlueprint::new`] with an explicit
    /// [`SupervisionStrategy::OneForAll`].
    pub fn one_for_all() -> Self {
        Self::new().strategy(SupervisionStrategy::OneForAll)
    }

    /// Same as [`SupervisorBlueprint::new`] with an explicit
    /// [`SupervisionStrategy::RestForOne`].
    pub fn rest_for_one() -> Self {
        Self::new().strategy(SupervisionStrategy::RestForOne)
    }

    /// Sets the [`SupervisionStrategy`] used to decide which children are
    /// restarted when one of them exits.
    pub fn strategy(mut self, strategy: SupervisionStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// In-place version of [`SupervisorBlueprint::strategy`].
    pub fn set_strategy(&mut self, strategy: SupervisionStrategy) {
        self.strategy = strategy;
    }

    /// Sets the supervisor-wide [`RestartIntensity`] restart budget. When
    /// unset, a strategy-dependent fallback is applied at instantiation.
    pub fn intensity(mut self, restart_intensity: RestartIntensity) -> Self {
        self.restart_intensity = Some(restart_intensity);
        self
    }

    /// In-place version of [`SupervisorBlueprint::intensity`], allowing the
    /// intensity to be set (`Some`) or cleared (`None`).
    pub fn set_intensity(&mut self, restart_intensity: Option<RestartIntensity>) {
        self.restart_intensity = restart_intensity;
    }

    /// Adds a single [`ChildSpec`], type-erasing it.
    pub fn child<T: Start + Sync>(mut self, spec: ChildSpec<T>) -> Self {
        self.supervisees
            .insert(spec.name().clone(), spec.into_dyn());
        self
    }

    /// Adds several [`ChildSpec`]s of the same type, type-erasing each one.
    pub fn children<T: Start>(mut self, specs: impl IntoIterator<Item = ChildSpec<T>>) -> Self {
        for spec in specs {
            let spec = spec.into_dyn();
            self.supervisees.insert(spec.name().clone(), spec);
        }
        self
    }

    /// Adds a [`ChildSpec`] by mutable reference and returns the child's
    /// [`Address`], so the caller can keep a handle to it.
    pub fn add_child<T>(&mut self, spec: ChildSpec<T>) -> Address<<T::Actor as Actor>::Interface>
    where
        T: Blueprint + Send + Sync + 'static,
    {
        let address = spec.address().clone();

        self.add_dyn_child(spec.into_dyn());

        address
    }

    /// Attaches a dynamic [`SupervisorSource`] the supervisor reads children
    /// from at startup and at runtime.
    pub fn source<S: SupervisorSource>(mut self, source: Arc<S>) -> Self {
        self.source = Some(source);
        self
    }

    /// Adds an already type-erased [`ChildSpec`] by mutable reference.
    pub fn add_dyn_child(&mut self, spec: ChildSpec) {
        self.supervisees.insert(spec.name().clone(), spec);
    }

    /// Adds several already type-erased [`ChildSpec`]s by mutable reference.
    pub fn add_dyn_children(&mut self, specs: impl IntoIterator<Item = ChildSpec>) {
        for spec in specs {
            self.add_dyn_child(spec);
        }
    }

    /// Adds several [`ChildSpec`]s by mutable reference, returning their
    /// [`Address`]es.
    pub fn add_children<T>(
        &mut self,
        specs: impl IntoIterator<Item = ChildSpec<T>>,
    ) -> Vec<Address<<T::Actor as Actor>::Interface>>
    where
        T: Blueprint + Send + Sync + 'static,
    {
        specs
            .into_iter()
            .map(|blueprint| self.add_child(blueprint))
            .collect()
    }

    fn fallback_restart_intensity(&self) -> RestartIntensity {
        match self.strategy {
            SupervisionStrategy::OneForOne => RestartIntensity {
                max_restarts: (self.supervisees.len() * 2).try_into().unwrap_or(u16::MAX),
                within: Duration::from_mins(5),
            },
            SupervisionStrategy::OneForAll | SupervisionStrategy::RestForOne => {
                RestartIntensity::default()
            }
        }
    }
}

impl Blueprint for SupervisorBlueprint {
    type Actor = Supervisor;

    async fn instantiate(&self) -> rootcause::Result<Self::Actor> {
        Ok(Supervisor::new(
            SuperviseeMap::new(self.supervisees.values().cloned()),
            self.strategy,
            self.restart_intensity
                .clone()
                .unwrap_or_else(|| self.fallback_restart_intensity()),
            self.source.clone(),
        ))
    }

    fn default_abort_timeout(&self) -> Duration {
        Duration::from_mins(5)
    }

    fn default_init_timeout(&self) -> Duration {
        Duration::from_mins(5)
    }
}

impl Debug for SupervisorBlueprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SupervisorBlueprint")
            .field("supervisees", &self.supervisees)
            .field("strategy", &self.strategy)
            .field("restart_intensity", &self.restart_intensity)
            .finish()
    }
}

impl Default for SupervisorBlueprint {
    fn default() -> Self {
        Self::new()
    }
}
