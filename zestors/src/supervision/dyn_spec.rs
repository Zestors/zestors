use super::*;
use futures::Future;
use pin_project::pin_project;
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

/// A type-erased and boxed [`Specification`].
pub struct DynSpec(Pin<Box<dyn IsDynMultiSpec + Send>>);

impl DynSpec {
    pub fn new<S>(spec: S) -> Self
    where
        S: IntoSpecification,
        S::Spec: Send + 'static,
        S::Fut: Unpin + Send,
        S::Supervisee: Unpin + Send,
    {
        Self(Box::pin(DynMultiSpec::Spec(spec.into_spec())))
    }
}

impl Specification for DynSpec {
    type Ref = ();
    type Supervisee = DynSupervisee;
    type Fut = DynSpecFut;

    fn start(mut self) -> Self::Fut {
        self.0.as_mut()._start();
        DynSpecFut(Some(self.0))
    }

    fn start_timeout(&self) -> Duration {
        self.0._start_timeout()
    }
}

/// A type-erased and boxed [`Specification::Fut`].
pub struct DynSpecFut(Option<Pin<Box<dyn IsDynMultiSpec + Send>>>);

impl DynSpecFut {
    pub fn new<S>(fut: S::Fut) -> Self
    where
        S: Specification + Send + 'static,
        S::Fut: Unpin + Send,
        S::Supervisee: Unpin + Send,
    {
        Self(Some(Box::pin(DynMultiSpec::<S>::StartFut(fut))))
    }
}

impl Future for DynSpecFut {
    type Output = StartResult<DynSpec>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .as_mut()
            .unwrap()
            .as_mut()
            ._poll_start_fut(cx)
            .map(|res| match res {
                DynSupervisedStart::Failure => {
                    Err(StartError::Failure(DynSpec(self.0.take().unwrap())))
                }
                DynSupervisedStart::Success => Ok((DynSupervisee(self.0.take()), ())),
                DynSupervisedStart::Finished => Err(StartError::Finished),
                DynSupervisedStart::Unhandled(e) => Err(StartError::Unhandled(e)),
            })
    }
}

/// A type-erased and boxed [`Supervisee`].
pub struct DynSupervisee(Option<Pin<Box<dyn IsDynMultiSpec + Send>>>);

impl DynSupervisee {
    pub fn new<S>(supervisee: S::Supervisee) -> Self
    where
        S: Specification + Send + 'static,
        S::Fut: Unpin + Send,
        S::Supervisee: Unpin + Send,
    {
        Self(Some(Box::pin(DynMultiSpec::<S>::Supervised(supervisee))))
    }
}

impl Supervisee for DynSupervisee {
    type Spec = DynSpec;

    fn abort_timeout(self: Pin<&Self>) -> Duration {
        self.0.as_ref().unwrap().as_ref()._abort_timeout()
    }

    fn halt(mut self: Pin<&mut Self>) {
        self.0.as_mut().unwrap().as_mut()._halt()
    }

    fn abort(mut self: Pin<&mut Self>) {
        self.0.as_mut().unwrap().as_mut()._abort()
    }
}

impl Future for DynSupervisee {
    type Output = ExitResult<DynSpec>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .as_mut()
            .unwrap()
            .as_mut()
            ._poll_supervise_fut(cx)
            .map(|res| match res {
                DynSupervisedExit::Finished => Ok(None),
                DynSupervisedExit::Exit => Ok(Some(DynSpec(self.0.take().unwrap()))),
                DynSupervisedExit::Unhandled(e) => Err(e),
            })
    }
}

/// This trait is only used for the Box-types.
///
/// The reason for this trait is that this allows us to only allocate a box once: When the initial
/// spec is turned into a dynamic one. Afterwards, if it exits and restarts, no more boxing is
/// necessary.
trait IsDynMultiSpec {
    fn _start(self: Pin<&mut Self>);
    fn _start_timeout(&self) -> Duration;
    fn _poll_start_fut(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<DynSupervisedStart>;
    fn _poll_supervise_fut(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<DynSupervisedExit>;
    fn _abort(self: Pin<&mut Self>);
    fn _halt(self: Pin<&mut Self>);
    fn _abort_timeout(self: Pin<&Self>) -> Duration;
}

// todo: it should be possible to provide an implementation that does not require Unpin for
// the Fut and Supervisee.
#[pin_project]
enum DynMultiSpec<S: Specification> {
    Spec(S),
    StartFut(S::Fut),
    Supervised(S::Supervisee),
    Unhandled,
    Finished,
}

enum DynSupervisedStart {
    Failure,
    Success,
    Finished,
    Unhandled(BoxError),
}

enum DynSupervisedExit {
    Finished,
    Exit,
    Unhandled(BoxError),
}

impl<S> IsDynMultiSpec for DynMultiSpec<S>
where
    S: Specification,
    S::Fut: Unpin,
    S::Supervisee: Unpin,
{
    fn _start(mut self: Pin<&mut Self>) {
        let this = {
            let mut this = Self::Unhandled;
            std::mem::swap(&mut *self, &mut this);
            this
        };

        let Self::Spec(spec) = this else { panic!() };

        std::mem::swap(&mut *self, &mut Self::StartFut(spec.start()));
    }

    fn _start_timeout(&self) -> Duration {
        let Self::Spec(spec) = self else { panic!() };
        spec.start_timeout()
    }

    fn _poll_start_fut(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<DynSupervisedStart> {
        let Self::StartFut(fut) = &mut *self else { panic!() };
        Pin::new(fut).poll(cx).map(|res| match res {
            Err(StartError::Finished) => {
                std::mem::swap(&mut *self, &mut Self::Finished);
                DynSupervisedStart::Finished
            }
            Err(StartError::Failure(spec)) => {
                std::mem::swap(&mut *self, &mut Self::Spec(spec));
                DynSupervisedStart::Failure
            }
            Ok((supervisee, reference)) => {
                drop(reference);
                std::mem::swap(&mut *self, &mut Self::Supervised(supervisee));
                DynSupervisedStart::Success
            }
            Err(StartError::Unhandled(e)) => {
                std::mem::swap(&mut *self, &mut Self::Unhandled);
                DynSupervisedStart::Unhandled(e)
            }
        })
    }

    fn _poll_supervise_fut(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<DynSupervisedExit> {
        let Self::Supervised(supervisee) = &mut *self else { panic!() };
        Pin::new(supervisee).poll(cx).map(|res| match res {
            Ok(None) => {
                std::mem::swap(&mut *self, &mut Self::Finished);
                DynSupervisedExit::Finished
            }
            Ok(Some(spec)) => {
                std::mem::swap(&mut *self, &mut Self::Spec(spec));
                DynSupervisedExit::Exit
            }
            Err(e) => {
                std::mem::swap(&mut *self, &mut Self::Unhandled);
                DynSupervisedExit::Unhandled(e)
            }
        })
    }

    fn _abort(mut self: Pin<&mut Self>) {
        let Self::Supervised(supervisee) = &mut *self else { panic!() };
        Pin::new(supervisee).abort()
    }

    fn _halt(mut self: Pin<&mut Self>) {
        let Self::Supervised(supervisee) = &mut *self else { panic!() };
        Pin::new(supervisee).halt()
    }

    fn _abort_timeout(self: Pin<&Self>) -> Duration {
        let Self::Supervised(supervisee) = &*self else { panic!() };
        Pin::new(supervisee).abort_timeout()
    }
}
