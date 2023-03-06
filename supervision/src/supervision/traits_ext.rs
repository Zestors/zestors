use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::Future;
use pin_project::pin_project;
use tokio::sync::mpsc;

use crate::all::*;

pub trait SpecificationExt: Specification {
    // /// Returns a future that can be awaited to supervise the supervisee.
    // fn start_supervisor(self) -> SupervisorFut<Self> {
    //     SupervisorFut::new(self)
    // }

    // /// Spawns an actor that supervises the supervisee.
    // fn spawn_under_supervisor(self) -> (Child<SupervisionResult<Self>>, SupervisorRef)
    // where
    //     Self: Send + 'static,
    //     Self::Supervisee: Send,
    // {
    //     self.start_supervisor().spawn_supervisor()
    // }

    // / Creates a new spec, which supervises by spawning a new actor (supervisor).
    // fn into_supervisor_spec(self) -> SupervisorSpec<Self> {
    //     SupervisorSpec::new(self)
    // }

    // /// Whenever the supervisee is started, the ref is sent to the receiver.
    // fn send_refs_with(self, sender: mpsc::UnboundedSender<Self::Ref>) -> RefSenderSpec<Self> {
    //     RefSenderSpec::new_with_channel(self, sender)
    // }

    /// Whenever the supervisee is started, the map function is called on the reference.
    fn on_start<F, T>(self, map: F) -> OnStartSpec<Self, F, T>
    where
        F: FnMut(Self::Ref) -> T,
    {
        OnStartSpec::new(self, map)
    }

    fn into_dyn(self) -> BoxSpec<Self::Ref>
    where
        Self: Send + 'static,
        Self::Supervisee: Send,
    {
        BoxSpec::new(self)
    }
}
impl<T: Specification> SpecificationExt for T {}

pub trait SuperviseeExt: Supervisee {
    fn supervise(self) -> SupervisionFuture<Self::Spec> {
        SupervisionFuture(self)
    }
}
impl<S: Supervisee> SuperviseeExt for S {}

#[pin_project]
pub struct SupervisionFuture<S: Specification>(#[pin] S::Supervisee);

impl<S: Specification> SupervisionFuture<S> {
    pub fn cancel(self) -> S::Supervisee {
        self.0
    }
}

impl<S: Specification> Future for SupervisionFuture<S> {
    type Output = SupervisionResult<S>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().0.poll_supervise(cx)
    }
}
