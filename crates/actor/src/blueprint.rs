use crate::{Actor, ActorExt, RestartMode};
use rootcause::Report;
use std::{fmt::Debug, future::Future, time::Duration};
use zestors_runtime::errors::DuplicateNameError;

/// A reusable recipe for producing an [`Actor`], together with the default
/// settings a supervisor should use when starting/restarting it.
pub trait Blueprint: Debug + Send + Sync + 'static {
    /// The concrete [`Actor`] type this blueprint produces.
    type Actor: Actor;

    /// Produces a new instance of the actor, ready to be spawned.
    fn instantiate(&self) -> impl Future<Output = rootcause::Result<Self::Actor>> + Send;

    /// The default time allowed for [`Blueprint::instantiate`] to complete.
    fn default_instantiation_timeout(&self) -> Duration {
        Duration::from_millis(5_000)
    }

    /// The default time allowed for the actor's task to exit after it is aborted.
    fn default_abort_timeout(&self) -> Duration {
        Duration::from_millis(5_000)
    }

    /// The default time allowed for the actor to finish initializing.
    fn default_init_timeout(&self) -> Duration {
        Duration::from_millis(5_000)
    }

    /// The default [`RestartMode`] to use for this actor.
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

/// Returned by [`BlueprintExt::start`]: either instantiating the actor
/// from its blueprint failed, or the given [`Name`] was already registered -
/// the same failure [`zestors_runtime::spawn`] itself has.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// Instantiating the actor from its blueprint failed; see
    /// [`Blueprint::instantiate`].
    #[error("failed to instantiate actor from blueprint: {0}")]
    Instantiate(Report),

    /// The given [`Name`] is already registered; see
    /// [`zestors_runtime::spawn`].
    #[error(transparent)]
    DuplicateName(#[from] DuplicateNameError),
}

/// Returned by [`BlueprintExt::start_rand`]: instantiating the actor
/// from its blueprint failed. Unlike [`StartError`], there's no
/// duplicate-name case, since [`zestors_runtime::spawn_rand`] always
/// generates a fresh [`Name`] and so can't fail that way.
#[derive(Debug, thiserror::Error)]
#[error("failed to instantiate actor from blueprint: {0}")]
pub struct StartRandError(pub Report);

/// Convenience methods for instantiating and spawning an [`Blueprint`]
/// in one step, mirroring [`zestors_runtime::spawn`]/[`zestors_runtime::spawn_rand`].
pub trait BlueprintExt: Blueprint {
    /// Instantiates this blueprint's actor and spawns it under `name`.
    fn start(
        &self,
        name: impl Into<Name>,
    ) -> impl Future<
        Output = Result<
            Child<<Self::Actor as Actor>::Exit, <Self::Actor as Actor>::Interface>,
            StartError,
        >,
    > + Send {
        let name = name.into();
        async move {
            let actor = self.instantiate().await.map_err(StartError::Instantiate)?;
            Ok(actor.spawn(name)?)
        }
    }

    /// Instantiates this blueprint's actor and spawns it under a freshly
    /// generated [`Name`].
    fn start_rand(
        &self,
    ) -> impl Future<
        Output = Result<
            Child<<Self::Actor as Actor>::Exit, <Self::Actor as Actor>::Interface>,
            StartRandError,
        >,
    > + Send {
        async move {
            let actor = self.instantiate().await.map_err(StartRandError)?;
            Ok(actor.spawn_rand())
        }
    }
}
impl<T: Blueprint> BlueprintExt for T {}

mod _hidden {
    use super::*;

    /// The [`Blueprint`] returned by [`fn_blueprint`](crate::fn_blueprint).
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
        /// Creates a new blueprint that instantiates the actor by calling `f`.
        pub fn new(f: F) -> Self {
            Self {
                f,
                restart_mode: RestartMode::default(),
            }
        }

        /// Sets the [`RestartMode`] returned by [`Blueprint::default_restart_mode`].
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
}
use _hidden::*;
use zestors_runtime::{Child, Name};

/// Creates an [`Blueprint`] that instantiates the actor by calling `f`.
pub fn fn_blueprint<F, A>(f: F) -> FnBlueprint<F, A>
where
    F: Fn() -> A + Send + Sync + 'static,
    A: Actor,
{
    FnBlueprint::new(f)
}
