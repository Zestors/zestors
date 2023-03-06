//! # Child specifications:
//!
//! ### Spawn specification 1:
//! - spawn_fn: async fn(D, Inbox) -> E
//! - exit_fn: async fn(ExitResult<E>) -> Result<Option<D>, BoxError>
//! - data: D
//! - inbox_config: Inbox::Config (default)
//! - abort_time: FnMut() -> Duration
//!
//! ### Spawn specification 2: (Clonable data)
//! - spawn_fn: async fn(D, Inbox) -> Exit
//! - exit_fn: async fn(ExitResult<Exit>) -> Result<bool, BoxError>
//! - data: D
//! - inbox_config: Inbox::Config (default)
//! - abort_time: FnMut() -> Duration
//!
//! ### Start specification 
//! - start_fn: async fn(D) -> Result<(Child<E, A>, Ref), StartError<Self>>
//! - exit_fn: async fn(ExitResult<E>) -> Result<Option<D>, BoxError>
//! - data: D
//! - abort_time: FnMut() -> Duration
//! 
//! ### Start specification 2:

pub(super) use super::*;
use std::time::Duration;
mod child;
mod spawn;
mod child_start_spec;
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
        exit: Result<Self::ProcessExit, ExitError>,
    ) -> Result<Option<Self::With>, FatalError>;

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(1)
    }

    fn into_spec(self, with: Self::With) -> RootSpec<Self> {
        RootSpec {
            spec: Box::new(self),
            with,
        }
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
    spec: Box<S>,
}
