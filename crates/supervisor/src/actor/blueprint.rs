use crate::{_prelude::*, actor::SuperviseeMap};
use indexmap::IndexMap;
use std::sync::Arc;
use zestors_supervision::{RestartIntensity, Start};

pub struct SupervisorBlueprint {
    supervisees: IndexMap<Pid, ChildSpec>,
    strategy: SupervisionStrategy,
    restart_intensity: Option<RestartIntensity>,
    source: Option<Arc<dyn SupervisorSource>>,
}

impl SupervisorBlueprint {
    pub fn new() -> Self {
        Self {
            supervisees: Default::default(),
            strategy: SupervisionStrategy::default(),
            restart_intensity: None,
            source: None,
        }
    }

    pub fn one_for_one() -> Self {
        Self::new().strategy(SupervisionStrategy::OneForOne)
    }

    pub fn one_for_all() -> Self {
        Self::new().strategy(SupervisionStrategy::OneForAll)
    }

    pub fn rest_for_one() -> Self {
        Self::new().strategy(SupervisionStrategy::RestForOne)
    }

    pub fn strategy(mut self, strategy: SupervisionStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn set_strategy(&mut self, strategy: SupervisionStrategy) {
        self.strategy = strategy;
    }

    pub fn intensity(mut self, restart_intensity: RestartIntensity) -> Self {
        self.restart_intensity = Some(restart_intensity);
        self
    }

    pub fn set_intensity(&mut self, restart_intensity: Option<RestartIntensity>) {
        self.restart_intensity = restart_intensity;
    }

    pub fn child<T: Start + Sync>(mut self, spec: ChildSpec<T>) -> Self {
        self.supervisees.insert(spec.pid().clone(), spec.into_dyn());
        self
    }

    pub fn children<T: Start>(mut self, specs: impl IntoIterator<Item = ChildSpec<T>>) -> Self {
        for spec in specs {
            let spec = spec.into_dyn();
            self.supervisees.insert(spec.pid().clone(), spec);
        }
        self
    }

    pub fn add_child<T>(&mut self, spec: ChildSpec<T>) -> Address<<T::Actor as Actor>::Interface>
    where
        T: ActorBlueprint + Send + Sync + 'static,
    {
        let address = spec.address().clone();

        self.add_dyn_child(spec.into_dyn());

        address
    }

    pub fn source<S: SupervisorSource>(mut self, source: Arc<S>) -> Self {
        self.source = Some(source);
        self
    }

    pub fn add_dyn_child(&mut self, spec: ChildSpec) {
        self.supervisees.insert(spec.pid().clone(), spec);
    }

    pub fn add_dyn_children(&mut self, specs: impl IntoIterator<Item = ChildSpec>) {
        for spec in specs {
            self.add_dyn_child(spec);
        }
    }

    pub fn add_children<T>(
        &mut self,
        specs: impl IntoIterator<Item = ChildSpec<T>>,
    ) -> Vec<Address<<T::Actor as Actor>::Interface>>
    where
        T: ActorBlueprint + Send + Sync + 'static,
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

impl ActorBlueprint for SupervisorBlueprint {
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
