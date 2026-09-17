use crate::{Actor, RestartIntensity, RestartMode};
use std::{fmt::Debug, future::Future, time::Duration};

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

mod _hidden {
    use super::*;

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
}
use _hidden::*;

pub fn fn_blueprint<F, A>(f: F) -> FnBlueprint<F, A>
where
    F: Fn() -> A + Send + Sync + 'static,
    A: Actor,
{
    FnBlueprint::new(f)
}
