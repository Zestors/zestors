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
    S: Startable,
    Fun: FnMut(S::Ref) -> T,
{
    spec: S,
    on_start: Fun,
}

#[pin_project]
pub struct MapRefStartFut<S, Fun, T>
where
    S: Startable,
    Fun: FnMut(S::Ref) -> T,
{
    #[pin]
    fut: S::Fut,
    on_start: Option<Fun>,
}

#[pin_project]
pub struct OnStartSupervisee<S, F, T>
where
    S: Startable,
    F: FnMut(S::Ref) -> T,
{
    #[pin]
    supervisee: S::Supervisee,
    on_start: Option<F>,
}

impl<S, F, T> MapRefSpec<S, F, T>
where
    S: Startable,
    F: FnMut(S::Ref) -> T,
{
    pub fn new(spec: S, map: F) -> Self {
        Self {
            spec,
            on_start: map,
        }
    }
}

impl<Sp, F, T> Startable for MapRefSpec<Sp, F, T>
where
    Sp: Startable,
    F: FnMut(Sp::Ref) -> T + Send + 'static,
    T: Send + 'static,
{
    type Ref = T;
    type Supervisee = OnStartSupervisee<Sp, F, T>;
    type Fut = MapRefStartFut<Sp, F, T>;

    fn start(self) -> Self::Fut {
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
    Sp: Startable,
    F: FnMut(Sp::Ref) -> T + Send + 'static,
    T: Send + 'static,
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

impl<S, F, T> Supervisable for OnStartSupervisee<S, F, T>
where
    S: Startable,
    F: FnMut(S::Ref) -> T + Send + 'static,
    T: Send + 'static,
{
    type Spec = MapRefSpec<S, F, T>;

    fn shutdown_time(self: Pin<&Self>) -> ShutdownTime {
        self.project_ref().supervisee.shutdown_time()
    }

    fn halt(self: Pin<&mut Self>) {
        self.project().supervisee.halt()
    }

    fn abort(self: Pin<&mut Self>) {
        self.project().supervisee.abort()
    }

    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context) -> Poll<SuperviseResult<Self::Spec>> {
        let proj = self.project();
        proj.supervisee.poll_supervise(cx).map(|res| {
            res.map(|spec| {
                spec.map(|spec| MapRefSpec {
                    spec,
                    on_start: proj.on_start.take().unwrap(),
                })
            })
        })
    }
}

