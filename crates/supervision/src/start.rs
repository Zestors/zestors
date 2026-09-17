use std::{fmt::Debug, sync::Arc};

use futures::future::BoxFuture;
use rootcause::Report;
use zestors_actor::{Actor, ActorBlueprint, ActorExt as _};
use zestors_runtime::{
    prelude::*,
    {AsDyn as _, Context, Dyn, IntoDyn, errors::ConcurrentInboxError},
};

/// Returned by [`Start::start_on`]: either the channel already had a
/// process running on it ([`ConcurrentInboxError`]), or instantiating the
/// actor from its blueprint failed.
#[derive(Debug, thiserror::Error)]
pub enum StartOnError {
    /// See [`ConcurrentInboxError`].
    #[error("There is already an active process running on this channel.")]
    ConcurrentInbox,

    /// Constructing the actor from its blueprint failed.
    #[error("Failed to instantiate actor from blueprint: {0}")]
    Instantiation(Report),
}

impl From<ConcurrentInboxError> for StartOnError {
    fn from(_: ConcurrentInboxError) -> Self {
        StartOnError::ConcurrentInbox
    }
}

/// A blueprint-like type that can spawn a task on an already-registered
/// [`StrongAddress`], rather than creating its own. Implemented
/// automatically for every [`ActorBlueprint`], and also implemented by the
/// type-erased [`DynStarter`].
///
/// This is what lets a [`ChildSpec`](crate::ChildSpec) hold on to its [`Pid`] and spawn (or
/// respawn) the same actor under it repeatedly.
pub trait Start: Into<DynStarter> {
    /// The [`Context`] of the actor this spawns.
    type Ctx: Context;

    /// The value produced once the spawned actor exits.
    type Exit: Send + 'static;

    /// Instantiates the actor and spawns it onto `channel`.
    fn start_on(
        &self,
        channel: StrongAddress<Self::Ctx>,
    ) -> impl Future<Output = Result<Child<Self::Exit, Self::Ctx>, StartOnError>> + Send;
}

impl<B: ActorBlueprint> Start for B {
    type Ctx = <B::Actor as Actor>::Interface;
    type Exit = <B::Actor as Actor>::Exit;

    async fn start_on(
        &self,
        channel: StrongAddress<Self::Ctx>,
    ) -> Result<Child<Self::Exit, Self::Ctx>, StartOnError> {
        let actor = self
            .instantiate()
            .await
            .map_err(StartOnError::Instantiation)?;

        channel.spawn(|inbox| actor.run(inbox)).map_err(Into::into)
    }
}

/// A type-erased [`ActorBlueprint`], used so a [`ChildSpec`](crate::ChildSpec) doesn't need to
/// carry its blueprint's concrete type. Created via [`DynStarter::new`], or
/// implicitly through [`Into<DynStarter>`] for any [`ActorBlueprint`].
#[derive(Debug, Clone)]
pub struct DynStarter(Arc<dyn Spawnable + Send + Sync + 'static>);

trait Spawnable: Debug {
    fn spawn_on_dyn<'a>(
        &'a self,
        data: &'a StrongAddress,
    ) -> BoxFuture<'a, Result<Child, StartOnError>>;
}

impl<R: ActorBlueprint> Spawnable for R {
    fn spawn_on_dyn<'a>(
        &'a self,
        data: &'a StrongAddress,
    ) -> BoxFuture<'a, Result<Child, StartOnError>> {
        Box::pin(async move {
            let runner = self
                .instantiate()
                .await
                .map_err(StartOnError::Instantiation)?
                .map_actor_exit(|res| res.map(|_| ()));

            data.downcast_ref::<<R::Actor as Actor>::Interface>()
                .expect("ProcessData should be of correct type")
                .clone()
                .spawn(|state| runner.run(state))
                .map(IntoDyn::into_dyn)
                .map_err(Into::into)
        })
    }
}

impl Start for DynStarter {
    type Ctx = Dyn;
    type Exit = ();

    async fn start_on(
        &self,
        data: StrongAddress<Self::Ctx>,
    ) -> Result<Child<Self::Exit, Self::Ctx>, StartOnError> {
        self.0.spawn_on_dyn(&data).await
    }
}

impl DynStarter {
    /// Type-erases `blueprint` into a `DynStarter`.
    pub fn new<R>(blueprint: R) -> Self
    where
        R: ActorBlueprint + Send + Sync + 'static,
    {
        DynStarter(Arc::new(blueprint))
    }
}

impl<R> From<R> for DynStarter
where
    R: ActorBlueprint + Send + Sync + 'static,
{
    fn from(value: R) -> Self {
        Self::new(value)
    }
}
