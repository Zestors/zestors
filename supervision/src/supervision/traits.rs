use super::FatalError;
#[allow(unused)]
use crate::all::*;
use async_trait::async_trait;
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

#[async_trait]
/// Specifies how [`Specification`] is started and supervised as a [`Supervisee`].
pub trait Specification: Send + Sized + 'static {
    /// The reference passed on when the supervisee is started
    type Ref: Send + 'static;

    /// The [`Supervisee`] returned after starting succesfully.
    type Supervisee: Supervisee<Spec = Self>;

    /// Start the supervisee.
    async fn start_supervised(self) -> StartResult<Self>;
}

#[derive(thiserror::Error)]
pub enum StartError<S> {
    /// Starting the supervisee has failed, but it may be retried.
    #[error("")]
    StartFailed(S),
    /// Starting the supervisee has failed, with no way to retry.
    #[error("")]
    Fatal(FatalError),
    /// The supervisee is completed and does not need to be restarted.
    #[error("")]
    Completed,
}

/// Returned when starting a [`Specification`].
pub type StartResult<S> =
    Result<(<S as Specification>::Supervisee, <S as Specification>::Ref), StartError<S>>;

/// Specifies how a [`Specification`] is supervised.
#[async_trait]
pub trait Supervisee: Send + Sized {
    type Spec: Specification<Supervisee = Self>;

    fn poll_supervise(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<SupervisionResult<Self::Spec>>;

    fn shutdown_time(self: Pin<&Self>) -> Duration;

    fn halt(self: Pin<&mut Self>);

    fn abort(self: Pin<&mut Self>);
}

/// Returned when a [`Supervisee`] exits.
pub type SupervisionResult<S> = Result<Option<S>, FatalError>;
