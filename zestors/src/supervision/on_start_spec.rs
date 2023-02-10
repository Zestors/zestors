use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use super::*;
use futures::{pin_mut, Future};
use pin_project::pin_project;

//------------------------------------------------------------------------------------------------
//  Spec
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct OnStartSpec<S, F, T>
where
    S: Specification,
    F: FnMut(S::Ref) -> T,
{
    spec: S,
    map: Option<F>,
}

#[pin_project]
pub struct OnStartSpecFut<S, F, T>
where
    S: Specification,
    F: FnMut(S::Ref) -> T,
{
    #[pin]
    fut: S::Fut,
    map: Option<F>,
}

#[pin_project]
pub struct OnStartSupervisee<S, F, T>
where
    S: Specification,
    F: FnMut(S::Ref) -> T,
{
    #[pin]
    supervisee: S::Supervisee,
    map: Option<F>,
}

impl<Sp, F, T> OnStartSpec<Sp, F, T>
where
    Sp: Specification,
    F: FnMut(Sp::Ref) -> T,
{
    pub fn new(spec: Sp, map: F) -> Self {
        Self {
            spec,
            map: Some(map),
        }
    }
}

impl<Sp, F, T> Specification for OnStartSpec<Sp, F, T>
where
    Sp: Specification,
    F: FnMut(Sp::Ref) -> T,
{
    type Ref = T;
    type Supervisee = OnStartSupervisee<Sp, F, T>;
    type Fut = OnStartSpecFut<Sp, F, T>;

    fn start(self) -> Self::Fut {
        OnStartSpecFut {
            fut: self.spec.start(),
            map: self.map,
        }
    }

    fn start_timeout(&self) -> Duration {
        self.spec.start_timeout()
    }
}

impl<Sp, F, T> Future for OnStartSpecFut<Sp, F, T>
where
    Sp: Specification,
    F: FnMut(Sp::Ref) -> T,
{
    type Output = StartResult<OnStartSpec<Sp, F, T>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let proj = self.project();
        proj.fut.poll(cx).map(|start| match start {
            Ok((supervisee, reference)) => {
                let mut map = proj.map.take().unwrap();
                let reference = map(reference);
                Ok((
                    OnStartSupervisee {
                        supervisee,
                        map: Some(map),
                    },
                    reference,
                ))
            }
            Err(StartError::Failure(spec)) => Err(StartError::Failure(OnStartSpec {
                spec,
                map: Some(proj.map.take().unwrap()),
            })),
            Err(StartError::Finished) => Err(StartError::Finished),
            Err(StartError::Unhandled(e)) => Err(StartError::Unhandled(e)),
        })
    }
}

//------------------------------------------------------------------------------------------------
//  Supervisee
//------------------------------------------------------------------------------------------------

impl<S, F, T> Supervisee for OnStartSupervisee<S, F, T>
where
    S: Specification,
    F: FnMut(S::Ref) -> T,
{
    type Spec = OnStartSpec<S, F, T>;

    fn abort_timeout(self: Pin<&Self>) -> Duration {
        self.project_ref().supervisee.abort_timeout()
    }

    fn halt(self: Pin<&mut Self>) {
        self.project().supervisee.halt()
    }

    fn abort(self: Pin<&mut Self>) {
        self.project().supervisee.abort()
    }
}

impl<S, F, T> Future for OnStartSupervisee<S, F, T>
where
    S: Specification,
    F: FnMut(S::Ref) -> T,
{
    type Output = ExitResult<OnStartSpec<S, F, T>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let proj = self.project();
        proj.supervisee.poll(cx).map(|res| {
            res.map(|spec| {
                spec.map(|spec| OnStartSpec {
                    spec,
                    map: proj.map.take(),
                })
            })
        })
    }
}
