use std::time::Duration;

pub(super) use super::*;

mod child;
mod spawn;
use async_trait::async_trait;
use futures::future::BoxFuture;

#[async_trait]
pub trait RootSpecification: 'static + Sized + Send {
    type ActorType: ActorType;
    type ProcessExit: Send + 'static;
    type Address;
    type With;

    async fn start(
        &mut self,
        with: Self::With,
    ) -> Result<(Child<Self::ProcessExit, Self::ActorType>, Self::Address), StartError<Self>>;

    async fn on_exit(
        &mut self,
        exit: Result<Self::ProcessExit, ProcessExitError>,
    ) -> Result<Option<Self::With>, BoxError>;

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(1)
    }

    fn into_spec(self, with: Self::With) -> RootSpec<Self> {
        RootSpec { spec: Box::new(self), with }
    }
}

pub struct RootSpec<S: RootSpecification> {
    spec: Box<S>,
    with: S::With,
}

pub struct RootSpecFut<S: RootSpecification> {
    spec: *mut Box<S>,
    fut: BoxFuture<
        'static,
        Result<(Child<S::ProcessExit, S::ActorType>, S::Address), StartError<S>>,
    >,
}

pub struct RootSupervisee<S: RootSpecification> {
    child: Child<S::ProcessExit, S::ActorType>,
    spec: Box<S>
}