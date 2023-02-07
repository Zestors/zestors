use async_trait::async_trait;
use tokio::sync::mpsc;

use super::*;
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

// pub trait SpecifiesChild: SpecifiesSupervisee {
//     type Ref;

//     fn poll_start_child(self) -> Poll<SuperviseeStart<(Self::Supervisee, Self::Ref), Self>>;
// }

/// let spec = Spec { function: spawn, val: u32 };
/// let (spec, address) = spec.clone().into_spec_with_rx();
/// let spec = spec.clone().into_spec_with_register(registry, "my_address");

//------------------------------------------------------------------------------------------------
//  ChildSpec
//------------------------------------------------------------------------------------------------

/// Specifies how a supervisee can be started and subsequently supervised with [`Supervisable`];
pub trait Spec: Unpin + Sized {
    /// The reference passed on when the supervisee is started
    type Ref;

    /// The [`Supervisable`] child returned when starting.
    type Supervisee: Supervisee<Spec = Self>;

    /// The method polled when trying starting the actor.
    /// After polling to completion, this may panic if called again.
    fn poll_start(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeStart<(Self::Supervisee, Self::Ref), Self>>;

    /// The limit for how much time is given for starting.
    /// After this time, the supervisee specification is dropped and an error is returned.
    ///
    /// This method should be callable multiple times.
    fn start_timeout(self: Pin<&Self>) -> Duration;
}

pub enum SuperviseeStart<S, Sp> {
    /// This actor is finished and does not need to be restarted.
    Finished,
    /// This actor failed to start and would like to be restarted.
    Failure(Sp),
    /// This actor successfully started and can now be supervised.
    Succesful(S),
    /// This actor has failed and cannot be restarted.
    UnrecoverableFailure(BoxError),
}

impl<S, Sp> SuperviseeStart<S, Sp> {
    pub fn map_failure<T>(self, map: impl FnOnce(Sp) -> T) -> SuperviseeStart<S, T> {
        match self {
            SuperviseeStart::Finished => SuperviseeStart::Finished,
            SuperviseeStart::Failure(spec) => SuperviseeStart::Failure(map(spec)),
            SuperviseeStart::Succesful(supervisee) => SuperviseeStart::Succesful(supervisee),
            SuperviseeStart::UnrecoverableFailure(e) => SuperviseeStart::UnrecoverableFailure(e),
        }
    }

    pub fn map_success<T>(self, map: impl FnOnce(S) -> T) -> SuperviseeStart<T, Sp> {
        match self {
            SuperviseeStart::Finished => SuperviseeStart::Finished,
            SuperviseeStart::Failure(spec) => SuperviseeStart::Failure(spec),
            SuperviseeStart::Succesful(supervisee) => SuperviseeStart::Succesful(map(supervisee)),
            SuperviseeStart::UnrecoverableFailure(e) => SuperviseeStart::UnrecoverableFailure(e),
        }
    }
}

pub trait SpecExt: Spec {
    /// Creates a new supervision specification, which supervises the given supervisee specification
    /// under by spawning a new actor (supervisor).
    fn supervisor_spec(self) -> SupervisorSpec<Self> {
        SupervisorSpec::new(self)
    }

    /// Returns a future that can be awaited to supervise the supervisee.
    fn supervise(self) -> SupervisionFut<Self> {
        SupervisionFut::new(self)
    }

    /// Spawns an actor that supervises the supervisee.
    fn spawn_supervisor(self) -> (Child<SuperviseeExit<Self>>, SupervisorRef)
    where
        Self: Send + 'static,
        Self::Supervisee: Send,
    {
        self.supervise().spawn_supervisor()
    }

    /// Whenever the supervisee is started, the ref is sent to the receiver.
    fn send_refs_with(self, sender: mpsc::UnboundedSender<Self::Ref>) -> RefSenderSpec<Self> {
        RefSenderSpec::new_with_channel(self, sender)
    }

    /// Whenever the supervisee is started, the map function is called on the reference.
    fn on_start<Fun: FnMut(Self::Ref) -> T, T>(self, map: Fun) -> MapRefSpec<Self, Fun, T> {
        MapRefSpec::new(self, map)
    }

    fn into_dyn(self) -> DynSpec
    where
        Self: Send + 'static,
        Self::Supervisee: Send,
    {
        Box::pin(self)
    }
}
impl<T: Spec> SpecExt for T {}

//------------------------------------------------------------------------------------------------
//  Supervisee
//------------------------------------------------------------------------------------------------

/// Specifies how a child can be supervised
pub trait Supervisee: Unpin + Sized {
    type Spec: Spec<Supervisee = Self>;

    fn poll_supervise(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeExit<Self::Spec>>;

    fn abort_timeout(self: Pin<&Self>) -> Duration;

    fn halt(self: Pin<&mut Self>);

    fn abort(self: Pin<&mut Self>);
}

pub enum SuperviseeExit<CS> {
    /// This actor is finished and does not need to be restarted.
    Finished,
    /// This actor has exited and would like to be restarted.
    Exit(CS),
    /// This actor has exited with a failure and cannot be restarted.
    UnrecoverableFailure(BoxError),
}

impl<Sp> SuperviseeExit<Sp> {
    pub fn map_exit<T>(self, map: impl FnOnce(Sp) -> T) -> SuperviseeExit<T> {
        match self {
            SuperviseeExit::Finished => SuperviseeExit::Finished,
            SuperviseeExit::Exit(spec) => SuperviseeExit::Exit(map(spec)),
            SuperviseeExit::UnrecoverableFailure(e) => SuperviseeExit::UnrecoverableFailure(e),
        }
    }
}

//------------------------------------------------------------------------------------------------
//  Dynamic childspec
//------------------------------------------------------------------------------------------------

pub trait DynSpecifiesSupervisee: Send + 'static {
    fn poll_start(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeStart<DynSupervisee, DynSpec>>;
    fn start_timeout(self: Pin<&Self>) -> Duration;
}

impl<T> DynSpecifiesSupervisee for T
where
    T: Spec + Send + 'static,
    T::Supervisee: Send + 'static,
{
    fn poll_start(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeStart<DynSupervisee, DynSpec>> {
        <Self as Spec>::poll_start(self, cx).map(|res| match res {
            SuperviseeStart::Finished => SuperviseeStart::Finished,
            SuperviseeStart::UnrecoverableFailure(e) => SuperviseeStart::UnrecoverableFailure(e),
            SuperviseeStart::Failure(cs) => {
                SuperviseeStart::Failure(Box::pin(cs) as Pin<Box<dyn DynSpecifiesSupervisee>>)
            }
            SuperviseeStart::Succesful(s) => {
                SuperviseeStart::Succesful(Box::pin(s.0) as Pin<Box<dyn DynSupervisable>>)
            }
        })
    }

    fn start_timeout(self: Pin<&Self>) -> Duration {
        <Self as Spec>::start_timeout(self)
    }
}

pub type DynSpec = Pin<Box<dyn DynSpecifiesSupervisee>>;

impl Spec for DynSpec {
    type Ref = ();
    type Supervisee = DynSupervisee;

    fn poll_start(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeStart<(Self::Supervisee, ()), Self>> {
        <Self as DynSpecifiesSupervisee>::poll_start(self, cx).map(|res| match res {
            SuperviseeStart::Finished => SuperviseeStart::Finished,
            SuperviseeStart::UnrecoverableFailure(e) => SuperviseeStart::UnrecoverableFailure(e),
            SuperviseeStart::Failure(cs) => {
                SuperviseeStart::Failure(Box::pin(cs) as Pin<Box<dyn DynSpecifiesSupervisee>>)
            }
            SuperviseeStart::Succesful(s) => {
                SuperviseeStart::Succesful((Box::pin(s) as Pin<Box<dyn DynSupervisable>>, ()))
            }
        })
    }

    fn start_timeout(self: Pin<&Self>) -> Duration {
        <Self as DynSpecifiesSupervisee>::start_timeout(self)
    }
}

//------------------------------------------------------------------------------------------------
//  Dynamic supervisee
//------------------------------------------------------------------------------------------------

pub trait DynSupervisable: Send + 'static {
    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<SuperviseeExit<DynSpec>>;
    fn abort(self: Pin<&mut Self>);
    fn halt(self: Pin<&mut Self>);
    fn abort_timeout(self: Pin<&Self>) -> Duration;
}

impl<T> DynSupervisable for T
where
    T: Supervisee + Send + 'static,
    T::Spec: Send + 'static,
{
    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<SuperviseeExit<DynSpec>> {
        <Self as Supervisee>::poll_supervise(self, cx).map(|res| match res {
            SuperviseeExit::Finished => SuperviseeExit::Finished,
            SuperviseeExit::UnrecoverableFailure(e) => SuperviseeExit::UnrecoverableFailure(e),
            SuperviseeExit::Exit(cs) => {
                SuperviseeExit::Exit(Box::pin(cs) as Pin<Box<dyn DynSpecifiesSupervisee>>)
            }
        })
    }

    fn abort(self: Pin<&mut Self>) {
        <Self as Supervisee>::abort(self)
    }

    fn halt(self: Pin<&mut Self>) {
        <Self as Supervisee>::halt(self)
    }

    fn abort_timeout(self: Pin<&Self>) -> Duration {
        <Self as Supervisee>::abort_timeout(self)
    }
}

pub type DynSupervisee = Pin<Box<dyn DynSupervisable>>;

impl Supervisee for DynSupervisee {
    type Spec = DynSpec;

    fn poll_supervise(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeExit<Self::Spec>> {
        <Self as DynSupervisable>::poll_supervise(self, cx)
    }

    fn halt(self: Pin<&mut Self>) {
        <Self as DynSupervisable>::halt(self)
    }

    fn abort(self: Pin<&mut Self>) {
        <Self as DynSupervisable>::abort(self)
    }

    fn abort_timeout(self: Pin<&Self>) -> Duration {
        <Self as DynSupervisable>::abort_timeout(self)
    }
}
