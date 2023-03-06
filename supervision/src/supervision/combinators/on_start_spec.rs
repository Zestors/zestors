use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use super::*;
use async_trait::async_trait;
use pin_project::pin_project;

//------------------------------------------------------------------------------------------------
//  Specification
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct OnStartSpec<S, Fun, T> {
    inner_spec: S,
    on_start: Fun,
    phantom: PhantomData<fn() -> T>,
}

impl<S, F, T> OnStartSpec<S, F, T>
where
    S: Specification,
    F: FnMut(S::Ref) -> T,
{
    pub fn new(spec: S, on_start: F) -> Self {
        Self {
            inner_spec: spec,
            on_start,
            phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<Sp, F, T> Specification for OnStartSpec<Sp, F, T>
where
    Sp: Specification,
    F: FnMut(Sp::Ref) -> T + Send + 'static,
    T: Send + 'static,
{
    type Ref = T;
    type Supervisee = OnStartSupervisee<Sp, F, T>;

    async fn start_supervised(mut self) -> StartResult<Self> {
        match self.inner_spec.start_supervised().await {
            Ok((supervisee, reference)) => {
                let reference = (self.on_start)(reference);
                Ok((
                    OnStartSupervisee {
                        supervisee,
                        on_start: Some(self.on_start),
                        phantom: self.phantom,
                    },
                    reference,
                ))
            }
            Err(StartError::StartFailed(inner_spec)) => {
                Err(StartError::StartFailed(OnStartSpec {
                    inner_spec,
                    on_start: self.on_start,
                    phantom: self.phantom,
                }))
            }
            Err(StartError::Completed) => Err(StartError::Completed),
            Err(StartError::Fatal(e)) => {
                Err(StartError::Fatal(e))
            }
        }
    }
}

//------------------------------------------------------------------------------------------------
//  Supervisee
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct OnStartSupervisee<S, F, T>
where
    S: Specification,
{
    #[pin]
    supervisee: S::Supervisee,
    on_start: Option<F>,
    phantom: PhantomData<fn() -> T>,
}

impl<S, F, T> Supervisee for OnStartSupervisee<S, F, T>
where
    S: Specification,
    F: FnMut(S::Ref) -> T + Send + 'static,
    T: Send + 'static,
{
    type Spec = OnStartSpec<S, F, T>;

    fn shutdown_time(self: Pin<&Self>) -> Duration {
        self.project_ref().supervisee.shutdown_time()
    }

    fn halt(self: Pin<&mut Self>) {
        self.project().supervisee.halt()
    }

    fn abort(self: Pin<&mut Self>) {
        self.project().supervisee.abort()
    }

    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context) -> Poll<SupervisionResult<Self::Spec>> {
        let this = self.project();
        this.supervisee.poll_supervise(cx).map(|res| {
            res.map(|spec| {
                spec.map(|spec| OnStartSpec {
                    inner_spec: spec,
                    on_start: this.on_start.take().unwrap(),
                    phantom: *this.phantom,
                })
            })
        })
    }
}
