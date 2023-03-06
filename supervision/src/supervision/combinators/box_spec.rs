use super::*;
use async_trait::async_trait;
use futures::{future::BoxFuture, Future};
use pin_project::pin_project;
use std::{
    fmt::Debug,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

#[derive(Debug)]
pub struct BoxSpec<Ref = ()>(Pin<Box<dyn DynSpecification<Ref>>>);

impl<Ref: 'static> BoxSpec<Ref> {
    pub fn new<S: Specification<Ref = Ref> + 'static>(spec: S) -> Self {
        Self(Box::pin(MultiSpec::Spec(spec)))
    }
}

#[async_trait]
impl<Ref: Send + 'static> Specification for BoxSpec<Ref> {
    type Ref = Ref;
    type Supervisee = BoxSupervisee<Ref>;

    async fn start_supervised(mut self) -> StartResult<Self> {
        self.0.as_mut()._start();
        BoxStartFut(Some(self.0)).await
    }
}

/// A type-erased and boxed [`Specification::StartFut`].
#[derive(Debug)]
struct BoxStartFut<Ref = ()>(Option<Pin<Box<dyn DynSpecification<Ref>>>>);

impl<Ref: Send + 'static> Future for BoxStartFut<Ref> {
    type Output = StartResult<BoxSpec<Ref>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .as_mut()
            .unwrap()
            .as_mut()
            ._poll_start_fut(cx)
            .map(|res| match res {
                DynSupervisedStart::Failure => Err(StartError::StartFailed(BoxSpec(
                    self.0.take().unwrap(),
                ))),
                DynSupervisedStart::Success(reference) => {
                    Ok((BoxSupervisee(self.0.take()), reference))
                }
                DynSupervisedStart::Finished => Err(StartError::Completed),
                DynSupervisedStart::Unhandled(e) => Err(StartError::Fatal(e)),
            })
    }
}

/// A type-erased and boxed [`Supervisee`].
#[derive(Debug)]
pub struct BoxSupervisee<Ref = ()>(Option<Pin<Box<dyn DynSpecification<Ref>>>>);

impl<Ref: 'static> BoxSupervisee<Ref> {
    pub fn new<S: Specification<Ref = Ref> + 'static>(supervisee: S::Supervisee) -> Self {
        Self(Some(Box::pin(MultiSpec::<S>::Supervised(supervisee))))
    }
}

impl<Ref: Send + 'static> Supervisee for BoxSupervisee<Ref> {
    type Spec = BoxSpec<Ref>;

    fn shutdown_time(self: Pin<&Self>) -> Duration {
        self.0.as_ref().unwrap().as_ref()._abort_timeout()
    }

    fn halt(mut self: Pin<&mut Self>) {
        self.0.as_mut().unwrap().as_mut()._halt()
    }

    fn abort(mut self: Pin<&mut Self>) {
        self.0.as_mut().unwrap().as_mut()._abort()
    }

    fn poll_supervise(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<SupervisionResult<Self::Spec>> {
        self.0
            .as_mut()
            .unwrap()
            .as_mut()
            ._poll_supervise_fut(cx)
            .map(|res| match res {
                DynSupervisedExit::Finished => Ok(None),
                DynSupervisedExit::Exit => Ok(Some(BoxSpec(self.0.take().unwrap()))),
                DynSupervisedExit::Unhandled(e) => Err(e),
            })
    }
}

/// This trait is only used for the Box-types.
///
/// The reason for this trait is that this allows us to only allocate a box once: When the initial
/// spec is turned into a dynamic one. Afterwards, if it exits and restarts, no more boxing is
/// necessary.
trait DynSpecification<Ref>: Send + Debug + 'static {
    fn _start(self: Pin<&mut Self>);
    fn _poll_start_fut(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<DynSupervisedStart<Ref>>;
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
    StartFut(#[pin] BoxFuture<'static, StartResult<S>>),
    Supervised(#[pin] S::Supervisee),
    Unhandled,
    Finished,
    SpecTaken,
}

impl<S: Specification> Debug for MultiSpec<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spec(_arg0) => f.debug_tuple("Spec").finish(),
            Self::StartFut(_arg0) => f.debug_tuple("StartFut").finish(),
            Self::Supervised(_arg0) => f.debug_tuple("Supervised").finish(),
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

enum DynSupervisedStart<Ref> {
    Failure,
    Success(Ref),
    Finished,
    Unhandled(FatalError),
}

enum DynSupervisedExit {
    Finished,
    Exit,
    Unhandled(FatalError),
}

impl<S> DynSpecification<S::Ref> for MultiSpec<S>
where
    S: Specification + 'static,
    S::Ref: 'static,
{
    fn _start(mut self: Pin<&mut Self>) {
        let spec = self.take_spec_unwrap();
        self.set(Self::StartFut(spec.start_supervised()));
    }

    fn _poll_start_fut(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<DynSupervisedStart<S::Ref>> {
        let this = self.as_mut().project();
        let DynMultiSpecProj::StartFut(fut) = this else { panic!() };

        fut.poll(cx).map(|res| match res {
            Err(StartError::Completed) => {
                self.set(Self::Finished);
                DynSupervisedStart::Finished
            }
            Err(StartError::StartFailed(spec)) => {
                self.set(Self::Spec(spec));
                DynSupervisedStart::Failure
            }
            Ok((supervisee, reference)) => {
                self.set(Self::Supervised(supervisee));
                DynSupervisedStart::Success(reference)
            }
            Err(StartError::Fatal(e)) => {
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
        supervisee.poll_supervise(cx).map(|res| match res {
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
