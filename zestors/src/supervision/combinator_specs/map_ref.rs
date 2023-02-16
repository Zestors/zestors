use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use super::*;
use futures::Future;
use pin_project::pin_project;

//------------------------------------------------------------------------------------------------
//  Spec
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct MapRefSpec<S, Fun, T>
where
    S: Specification,
    Fun: FnMut(S::Ref) -> T,
{
    spec: S,
    on_start: Fun,
}

#[pin_project]
pub struct MapRefStartFut<S, Fun, T>
where
    S: Specification,
    Fun: FnMut(S::Ref) -> T,
{
    #[pin]
    fut: S::StartFut,
    on_start: Option<Fun>,
}

#[pin_project]
pub struct OnStartSupervisee<S, F, T>
where
    S: Specification,
    F: FnMut(S::Ref) -> T,
{
    #[pin]
    supervisee: S::Supervisee,
    on_start: Option<F>,
}

impl<S, F, T> MapRefSpec<S, F, T>
where
    S: Specification,
    F: FnMut(S::Ref) -> T,
{
    pub fn new(spec: S, map: F) -> Self {
        Self {
            spec,
            on_start: map,
        }
    }
}

impl<Sp, F, T> Specification for MapRefSpec<Sp, F, T>
where
    Sp: Specification,
    F: FnMut(Sp::Ref) -> T + Send,
    T: Send,
{
    type Ref = T;
    type Supervisee = OnStartSupervisee<Sp, F, T>;
    type StartFut = MapRefStartFut<Sp, F, T>;

    fn start(self) -> Self::StartFut {
        MapRefStartFut {
            fut: self.spec.start(),
            on_start: Some(self.on_start),
        }
    }

    fn start_time(&self) -> Duration {
        self.spec.start_time()
    }
}

impl<Sp, F, T> Future for MapRefStartFut<Sp, F, T>
where
    Sp: Specification,
    F: FnMut(Sp::Ref) -> T + Send,
    T: Send,
{
    type Output = StartResult<MapRefSpec<Sp, F, T>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let proj = self.project();
        proj.fut.poll(cx).map(|start| match start {
            Ok((supervisee, reference)) => {
                let mut map = proj.on_start.take().unwrap();
                let reference = map(reference);
                Ok((
                    OnStartSupervisee {
                        supervisee,
                        on_start: Some(map),
                    },
                    reference,
                ))
            }
            Err(StartError::Failed(spec)) => Err(StartError::Failed(MapRefSpec {
                spec,
                on_start: proj.on_start.take().unwrap(),
            })),
            Err(StartError::Completed) => Err(StartError::Completed),
            Err(StartError::Irrecoverable(e)) => Err(StartError::Irrecoverable(e)),
        })
    }
}

//------------------------------------------------------------------------------------------------
//  Supervisee
//------------------------------------------------------------------------------------------------

impl<S, F, T> Supervisee for OnStartSupervisee<S, F, T>
where
    S: Specification,
    F: FnMut(S::Ref) -> T + Send,
    T: Send,
{
    type Spec = MapRefSpec<S, F, T>;

    fn shutdown_time(self: Pin<&Self>) -> Duration {
        self.project_ref().supervisee.shutdown_time()
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
    type Output = ExitResult<MapRefSpec<S, F, T>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let proj = self.project();
        proj.supervisee.poll(cx).map(|res| {
            res.map(|spec| {
                spec.map(|spec| MapRefSpec {
                    spec,
                    on_start: proj.on_start.take().unwrap(),
                })
            })
        })
    }
}
