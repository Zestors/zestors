use super::*;
use futures::Future;
use std::{pin::Pin, time::Duration};
use tokio::sync::mpsc;

pub type StartResult<S> =
    Result<(<S as Specification>::Supervisee, <S as Specification>::Ref), StartError<S>>;

pub enum StartError<S> {
    Unhandled(BoxError),
    Failure(S),
    Finished,
}

pub type ExitResult<S> = Result<Option<S>, BoxError>;

/// Specifies how a supervisee can be started and subsequently supervised with [`Supervisee`];
pub trait Specification: Sized {
    /// The reference passed on when the supervisee is started
    type Ref;

    /// The [`Supervisable`] child returned when starting.
    type Supervisee: Supervisee<Spec = Self>;

    type Fut: Future<Output = StartResult<Self>>;

    fn start(self) -> Self::Fut;

    /// The limit for how much time is given for starting.
    /// After this time, the supervisee specification is dropped and an error is returned.
    ///
    /// This method should be callable repeatedly.
    fn start_timeout(&self) -> Duration;
}

// pub enum Start<S: Specification> {
//     Finished,
//     Failure(S),
//     Success((S::Supervisee, S::Ref)),
//     Unhandled(BoxError),
// }

// impl<S: Specification> Start<S> {
//     pub fn map<T: Specification>(
//         self,
//         failure: impl FnOnce(S) -> T,
//         success: impl FnOnce((S::Supervisee, S::Ref)) -> (T::Supervisee, T::Ref),
//     ) -> Start<T> {
//         match self {
//             Start::Finished => Start::Finished,
//             Start::Failure(spec) => Start::Failure(failure(spec)),
//             Start::Success(supervisee) => Start::Success(success(supervisee)),
//             Start::Unhandled(e) => Start::Unhandled(e),
//         }
//     }
// }

//------------------------------------------------------------------------------------------------
//  IntoSpecification
//------------------------------------------------------------------------------------------------

pub trait IntoSpecification: Sized {
    type Spec: Specification<Ref = Self::Ref, Supervisee = Self::Supervisee, Fut = Self::Fut>;
    type Supervisee: Supervisee<Spec = Self::Spec>;
    type Ref;
    type Fut: Future<Output = StartResult<Self::Spec>>;

    fn into_spec(self) -> Self::Spec;
}

impl<T> IntoSpecification for T
where
    T: Specification,
{
    type Spec = Self;
    type Supervisee = T::Supervisee;
    type Ref = T::Ref;
    type Fut = T::Fut;

    fn into_spec(self) -> Self::Spec {
        self
    }
}

//------------------------------------------------------------------------------------------------
//  SpecificationExt
//------------------------------------------------------------------------------------------------

pub trait SpecificationExt: IntoSpecification {
    /// Returns a future that can be awaited to supervise the supervisee.
    fn supervise(self) -> SupervisorFut<Self::Spec> {
        SupervisorFut::new(self.into_spec())
    }

    /// Spawns an actor that supervises the supervisee.
    fn spawn_supervisor(self) -> (Child<ExitResult<Self::Spec>>, SupervisorRef)
    where
        Self::Spec: Send + 'static,
        Self::Supervisee: Send,
        Self::Fut: Send,
    {
        self.supervise().spawn_supervisor()
    }

    // / Creates a new spec, which supervises by spawning a new actor (supervisor).
    // fn into_supervisor_spec(self) -> SupervisorSpec<Self> {
    //     SupervisorSpec::new(self)
    // }

    /// Whenever the supervisee is started, the ref is sent to the receiver.
    fn send_refs_with(self, sender: mpsc::UnboundedSender<Self::Ref>) -> RefSenderSpec<Self::Spec> {
        RefSenderSpec::new_with_channel(self.into_spec(), sender)
    }

    /// Whenever the supervisee is started, the map function is called on the reference.
    fn on_start<F, T>(self, map: F) -> OnStartSpec<Self::Spec, F, T>
    where
        F: FnMut(Self::Ref) -> T,
    {
        OnStartSpec::new(self.into_spec(), map)
    }

    fn into_dyn(self) -> DynSpec
    where
        Self::Spec: Send + 'static,
        Self::Supervisee: Unpin + Send,
        Self::Fut: Unpin + Send,
    {
        DynSpec::new(self.into_spec())
    }
}
impl<T: IntoSpecification> SpecificationExt for T {}

//------------------------------------------------------------------------------------------------
//  Supervisee
//------------------------------------------------------------------------------------------------

/// Specifies how a child can be supervised
pub trait Supervisee: Future<Output = ExitResult<Self::Spec>> + Sized {
    type Spec: Specification<Supervisee = Self>;

    fn abort_timeout(self: Pin<&Self>) -> Duration;

    fn halt(self: Pin<&mut Self>);

    fn abort(self: Pin<&mut Self>);
}

// pub enum Exit<CS> {
//     /// This actor is finished and does not need to be restarted.
//     Finished,
//     /// This actor has exited and would like to be restarted.
//     Exit(CS),
//     /// This actor has exited with a failure and cannot be restarted.
//     Unhandled(BoxError),
// }

// impl<Sp> Exit<Sp> {
//     pub fn map<T>(self, map: impl FnOnce(Sp) -> T) -> Exit<T> {
//         match self {
//             Exit::Finished => Exit::Finished,
//             Exit::Exit(spec) => Exit::Exit(map(spec)),
//             Exit::Unhandled(e) => Exit::Unhandled(e),
//         }
//     }
// }
