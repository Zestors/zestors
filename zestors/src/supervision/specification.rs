use super::*;
use futures::Future;
use std::{pin::Pin, time::Duration};
use tokio::sync::mpsc;

/// A [`Specification`] specifies how [`Supervisee`] can be started.
pub trait Specification: Sized {
    /// The reference passed on when the supervisee is started
    type Ref;

    /// The [`Supervisee`] returned after starting succesfully.
    type Supervisee: Supervisee<Spec = Self>;

    /// The future returned when starting.
    type StartFut: Future<Output = StartResult<Self>>;

    /// Start the supervisee.
    fn start(self) -> Self::StartFut;

    /// The limit for how much time is given for starting.
    /// After this time, the supervisee specification is dropped and an error is returned.
    fn start_time(&self) -> Duration;
}

/// Specifies how a child can be supervised
pub trait Supervisee: Future<Output = ExitResult<Self::Spec>> + Sized {
    type Spec: Specification<Supervisee = Self>;

    fn shutdown_time(self: Pin<&Self>) -> Duration;

    fn halt(self: Pin<&mut Self>);

    fn abort(self: Pin<&mut Self>);
}

/// Result returned when starting a [`Specification`].
pub type StartResult<S> =
    Result<(<S as Specification>::Supervisee, <S as Specification>::Ref), StartError<S>>;

pub enum StartError<S> {
    /// Starting the supervisee has failed, but it may be retried.
    Failed(S),
    /// Starting the supervisee has failed, with no way to retry.
    Irrecoverable(BoxError),
    /// The supervisee is completed and does not need to be restarted.
    Completed,
}

/// Result returned when a [`Supervisee`] exits.
///
/// This may return:
/// - `Ok(None)` -> The supervisee is finished and does not want to be restarted.
/// - `Ok(Some(S))` -> The supervisee has exited and would like to be restarted.
/// - `Err(BoxError)` -> The supervisee has exited with an unhandled error, and can not be
///   restarted.
pub type ExitResult<S> = Result<Option<S>, BoxError>;

//------------------------------------------------------------------------------------------------
//  IntoSpecification
//------------------------------------------------------------------------------------------------

// pub trait IntoSpecification: Sized {
//     type Spec: Specification<Ref = Self::Ref, Supervisee = Self::Supervisee, StartFut = Self::StartFut>;
//     type Supervisee;
//     type Ref;
//     type StartFut;

//     fn into_spec(self) -> Self::Spec;
// }

// impl<T> IntoSpecification for T
// where
//     T: Specification,
// {
//     type Spec = Self;
//     type Supervisee = T::Supervisee;
//     type Ref = T::Ref;
//     type StartFut = T::StartFut;

//     fn into_spec(self) -> Self::Spec {
//         self
//     }
// }

//------------------------------------------------------------------------------------------------
//  SpecificationExt
//------------------------------------------------------------------------------------------------

pub trait SpecificationExt: Specification {
    /// Returns a future that can be awaited to supervise the supervisee.
    fn supervise(self) -> SupervisorFut<Self> {
        SupervisorFut::new(self)
    }

    /// Spawns an actor that supervises the supervisee.
    fn spawn_under_supervisor(self) -> (Child<ExitResult<Self>>, SupervisorRef)
    where
        Self: Send + 'static,
        Self::Supervisee: Send,
        Self::StartFut: Send,
    {
        self.supervise().spawn_supervisor()
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

    fn into_dyn(self) -> DynSpec
    where
        Self: Send + 'static,
        Self::Supervisee: Send,
        Self::StartFut: Send,
    {
        DynSpec::new(self)
    }
}
impl<T: Specification> SpecificationExt for T {}

//------------------------------------------------------------------------------------------------
//  Supervisee
//------------------------------------------------------------------------------------------------
