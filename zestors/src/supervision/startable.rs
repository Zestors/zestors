use super::*;
use futures::Future;
use pin_project::pin_project;
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::mpsc;

//------------------------------------------------------------------------------------------------
//  Startable
//------------------------------------------------------------------------------------------------

pub trait UnpinStartable: Startable + Unpin
where
    Self::Supervisee: Unpin,
    Self::Fut: Unpin,
{
}
impl<S> UnpinStartable for S
where
    S: Startable + Unpin,
    S::Supervisee: Unpin,
    S::Fut: Unpin,
{
}

/// A [`Specification`] specifies how [`Supervisee`] can be started.
pub trait Startable: Send + Sized {
    /// The reference passed on when the supervisee is started
    type Ref: Send + 'static;

    /// The [`Supervisee`] returned after starting succesfully.
    type Supervisee: Supervisable<Spec = Self>;

    /// The future returned when starting.
    type Fut: Future<Output = StartResult<Self>> + Send;

    /// Start the supervisee.
    fn start(self) -> Self::Fut;

    /// The limit for how much time is given for starting.
    /// After this time, the supervisee specification is dropped and an error is returned.
    fn start_time(&self) -> Duration;
}

pub trait StartableExt: Startable {
    /// Returns a future that can be awaited to supervise the supervisee.
    fn start_supervise(self) -> SupervisorFut<Self> {
        SupervisorFut::new(self)
    }

    /// Spawns an actor that supervises the supervisee.
    fn spawn_under_supervisor(self) -> (Child<SuperviseResult<Self>>, SupervisorRef)
    where
        Self: Send + 'static,
        Self::Supervisee: Send,
        Self::Fut: Send,
    {
        self.start_supervise().spawn_supervisor()
    }

    // / Creates a new spec, which supervises by spawning a new actor (supervisor).
    // fn into_supervisor_spec(self) -> SupervisorSpec<Self> {
    //     SupervisorSpec::new(self)
    // }

    /// Whenever the supervisee is started, the ref is sent to the receiver.
    fn send_refs_with(self, sender: mpsc::UnboundedSender<Self::Ref>) -> RefSenderSpec<Self> {
        RefSenderSpec::new_with_channel(self, sender)
    }

    /// Whenever the supervisee is started, the map function is called on the reference.
    fn on_start<F, T>(self, map: F) -> MapRefSpec<Self, F, T>
    where
        F: FnMut(Self::Ref) -> T,
    {
        MapRefSpec::new(self, map)
    }

    fn into_dyn(self) -> DynSpec<Self::Ref>
    where
        Self: Send + 'static,
        Self::Supervisee: Send,
        Self::Fut: Send,
    {
        DynSpec::new(self)
    }
}
impl<T: Startable> StartableExt for T {}

/// Result returned when starting a [`Specification`].
pub type StartResult<S> =
    Result<(<S as Startable>::Supervisee, <S as Startable>::Ref), StartError<S>>;

pub enum StartError<S> {
    /// Starting the supervisee has failed, but it may be retried.
    Failed(S),
    /// Starting the supervisee has failed, with no way to retry.
    Irrecoverable(BoxError),
    /// The supervisee is completed and does not need to be restarted.
    Completed,
}

//------------------------------------------------------------------------------------------------
//  Supervisable
//------------------------------------------------------------------------------------------------

/// Specifies how a child can be supervised
pub trait Supervisable: Send + Sized {
    type Spec: Startable<Supervisee = Self>;

    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context) -> Poll<SuperviseResult<Self::Spec>>;

    fn shutdown_time(self: Pin<&Self>) -> ShutdownTime;

    fn halt(self: Pin<&mut Self>);

    fn abort(self: Pin<&mut Self>);
}

/// Result returned when a [`Supervisee`] exits.
///
/// This may return:
/// - `Ok(None)` -> The supervisee is finished and does not want to be restarted.
/// - `Ok(Some(S))` -> The supervisee has exited and would like to be restarted.
/// - `Err(BoxError)` -> The supervisee has exited with an unhandled error, and can not be
///   restarted.
pub type SuperviseResult<S> = Result<Option<S>, BoxError>;

#[pin_project]
pub struct SuperviseFuture<S: Startable>(#[pin] S::Supervisee);

impl<S: Startable> Future for SuperviseFuture<S> {
    type Output = SuperviseResult<S>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().0.poll_supervise(cx)
    }
}

pub trait SupervisableExt: Supervisable {
    fn supervise(self) -> SuperviseFuture<Self::Spec> {
        SuperviseFuture(self)
    }
}
impl<S: Supervisable> SupervisableExt for S {}
