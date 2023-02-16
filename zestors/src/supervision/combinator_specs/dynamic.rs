use super::*;
use futures::Future;
use pin_project::pin_project;
use std::{
    fmt::Debug,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

/// A type-erased and boxed [`Specification`].
#[derive(Debug)]
pub struct DynSpec(Pin<Box<dyn DynSpecification + Send>>);

impl DynSpec {
    pub fn new<S>(spec: S) -> Self
    where
        S: Specification + Send + 'static,
        S::StartFut: Send,
        S::Supervisee: Send,
    {
        Self(Box::pin(MultiSpec::Spec(spec)))
    }
}

impl Specification for DynSpec {
    type Ref = ();
    type Supervisee = DynSupervisee;
    type StartFut = DynStartFut;

    fn start(mut self) -> Self::StartFut {
        self.0.as_mut()._start();
        DynStartFut(Some(self.0))
    }

    fn start_time(&self) -> Duration {
        self.0._start_timeout()
    }
}

/// A type-erased and boxed [`Specification::StartFut`].
#[derive(Debug)]
pub struct DynStartFut(Option<Pin<Box<dyn DynSpecification + Send>>>);

impl DynStartFut {
    pub fn new<S>(fut: S::StartFut) -> Self
    where
        S: Specification + Send + 'static,
        S::StartFut: Send,
        S::Supervisee: Send,
    {
        Self(Some(Box::pin(MultiSpec::<S>::StartFut(fut))))
    }
}

impl Future for DynStartFut {
    type Output = StartResult<DynSpec>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .as_mut()
            .unwrap()
            .as_mut()
            ._poll_start_fut(cx)
            .map(|res| match res {
                DynSupervisedStart::Failure => {
                    Err(StartError::Failed(DynSpec(self.0.take().unwrap())))
                }
                DynSupervisedStart::Success => Ok((DynSupervisee(self.0.take()), ())),
                DynSupervisedStart::Finished => Err(StartError::Completed),
                DynSupervisedStart::Unhandled(e) => Err(StartError::Irrecoverable(e)),
            })
    }
}

/// A type-erased and boxed [`Supervisee`].
#[derive(Debug)]
pub struct DynSupervisee(Option<Pin<Box<dyn DynSpecification + Send>>>);

impl DynSupervisee {
    pub fn new<S>(supervisee: S::Supervisee) -> Self
    where
        S: Specification + Send + 'static,
        S::StartFut: Send,
        S::Supervisee: Send,
    {
        Self(Some(Box::pin(MultiSpec::<S>::Supervised(supervisee))))
    }
}

impl Supervisee for DynSupervisee {
    type Spec = DynSpec;

    fn shutdown_time(self: Pin<&Self>) -> Duration {
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
trait DynSpecification: Debug {
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
#[pin_project(project = DynMultiSpecProj, project_ref = DynMultiSpecProjRef)]
enum MultiSpec<S: Specification> {
    Spec(S),
    StartFut(#[pin] S::StartFut),
    Supervised(#[pin] S::Supervisee),
    Unhandled,
    Finished,
    SpecTaken,
}

impl<S: Specification> Debug for MultiSpec<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spec(arg0) => f.debug_tuple("Spec").finish(),
            Self::StartFut(arg0) => f.debug_tuple("StartFut").finish(),
            Self::Supervised(arg0) => f.debug_tuple("Supervised").finish(),
            Self::Unhandled => write!(f, "Unhandled"),
            Self::Finished => write!(f, "Finished"),
            Self::SpecTaken => write!(f, "SpecTaken"),
        }
    }
}

impl<S: Specification> MultiSpec<S> {
    fn take_spec_unwrap(self: &mut Pin<&mut Self>) -> S {
        let mut taken = Self::SpecTaken;
        std::mem::swap(unsafe { self.as_mut().get_unchecked_mut() }, &mut taken);
        let Self::Spec(taken_spec) = taken else { panic!() };
        taken_spec
    }
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

impl<S> DynSpecification for MultiSpec<S>
where
    S: Specification,
{
    fn _start(mut self: Pin<&mut Self>) {
        let spec = self.take_spec_unwrap();
        self.set(Self::StartFut(spec.start()));
    }

    fn _start_timeout(&self) -> Duration {
        let Self::Spec(spec) = self else { panic!() };
        spec.start_time()
    }

    fn _poll_start_fut(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<DynSupervisedStart> {
        let this = self.as_mut().project();
        let DynMultiSpecProj::StartFut(fut) = this else { panic!() };

        fut.poll(cx).map(|res| match res {
            Err(StartError::Completed) => {
                self.set(Self::Finished);
                DynSupervisedStart::Finished
            }
            Err(StartError::Failed(spec)) => {
                self.set(Self::Spec(spec));
                DynSupervisedStart::Failure
            }
            Ok((supervisee, reference)) => {
                drop(reference);
                self.set(Self::Supervised(supervisee));
                DynSupervisedStart::Success
            }
            Err(StartError::Irrecoverable(e)) => {
                self.set(Self::Unhandled);
                DynSupervisedStart::Unhandled(e)
            }
        })
    }

    fn _poll_supervise_fut(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<DynSupervisedExit> {
        let this = self.as_mut().project();
        let DynMultiSpecProj::Supervised(supervisee) = this else { panic!() };
        supervisee.poll(cx).map(|res| match res {
            Ok(None) => {
                self.set(Self::Finished);
                DynSupervisedExit::Finished
            }
            Ok(Some(spec)) => {
                self.set(Self::Spec(spec));
                DynSupervisedExit::Exit
            }
            Err(e) => {
                self.set(Self::Unhandled);
                DynSupervisedExit::Unhandled(e)
            }
        })
    }

    fn _abort(self: Pin<&mut Self>) {
        let DynMultiSpecProj::Supervised(supervisee) = self.project() else { panic!() };
        supervisee.abort()
    }

    fn _halt(self: Pin<&mut Self>) {
        let DynMultiSpecProj::Supervised(supervisee) = self.project() else { panic!() };
        supervisee.halt()
    }

    fn _abort_timeout(self: Pin<&Self>) -> Duration {
        let DynMultiSpecProjRef::Supervised(supervisee) = self.project_ref() else { panic!() };
        supervisee.shutdown_time()
    }
}
