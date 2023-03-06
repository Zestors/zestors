use super::{get_default_shutdown_time, FatalError};
use crate::all::*;
use async_trait::async_trait;
use futures::{future::BoxFuture, Future, FutureExt};
use pin_project::pin_project;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{ready, Context, Poll},
    time::Duration,
};

pub struct BoxStartChildSpec<E: Send + 'static, A: ActorType, Ref> {
    start_fn: Box<
        dyn FnOnce(
            Option<Result<E, ExitError>>,
        ) -> BoxFuture<'static, Result<(Child<E, A>, Ref), FatalError>>,
    >,
    exit_fn: Box<
        dyn FnOnce(
            Option<Result<E, ExitError>>,
        ) -> BoxFuture<'static, Result<(Child<E, A>, Ref), FatalError>>,
    >,
}

#[pin_project]
pub struct StartChildSpec<SFun, SFut, EFun, EFut, T, E, A, Ref> {
    start_fn: SFun,
    exit_fn: EFun,
    with: T,
    phantom: PhantomData<(SFut, EFut, E, A, Ref)>,
}

impl<SFun, SFut, EFun, EFut, T, E, A, Ref> StartChildSpec<SFun, SFut, EFun, EFut, T, E, A, Ref> {
    pub fn new(start_fn: SFun, exit_fn: EFun, with: T) -> Self {
        Self {
            start_fn,
            exit_fn,
            with,
            phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<SFun, SFut, EFun, EFut, T, E, A, Ref> Specification
    for StartChildSpec<SFun, SFut, EFun, EFut, T, E, A, Ref>
where
    SFun: FnOnce(T) -> SFut + Send + Clone + 'static,
    SFut: Future<
            Output = Result<
                (Child<E, A>, Ref),
                StartError<StartChildSpec<SFun, SFut, EFun, EFut, T, E, A, Ref>>,
            >,
        > + Send
        + 'static,
    EFun: (FnOnce(Result<E, ExitError>) -> EFut) + Send + Clone + 'static,
    EFut: Future<Output = SupervisionResult<T>> + Send + 'static,
    E: Send + 'static,
    T: Send + 'static,
    A: ActorType + Send + 'static,
    Ref: Send + 'static,
{
    type Ref = Ref;
    type Supervisee = StartChildSupervisee<SFun, SFut, EFun, EFut, T, E, A, Ref>;

    async fn start_supervised(self) -> StartResult<Self> {
        let start_fut = (self.start_fn.clone())(self.with);
        match start_fut.await {
            Ok((child, reference)) => {
                let supervisee = StartChildSupervisee {
                    child,
                    start_fn: Some(self.start_fn),
                    exit_fn: Some(self.exit_fn),
                    phantom: PhantomData,
                    exit_fut: None,
                };
                Ok((supervisee, reference))
            }
            Err(e) => Err(e),
        }
    }
}

#[pin_project]
pub struct StartChildSupervisee<SFun, SFut, EFun, EFut, D, E, A, Ref>
where
    E: Send + 'static,
    A: ActorType,
{
    child: Child<E, A>,
    start_fn: Option<SFun>,
    exit_fn: Option<EFun>,
    #[pin]
    exit_fut: Option<EFut>,
    phantom: PhantomData<(SFut, EFut, D, Ref)>,
}

impl<SFun, SFut, EFun, EFut, T, E, A, Ref> Supervisee
    for StartChildSupervisee<SFun, SFut, EFun, EFut, T, E, A, Ref>
where
    SFun: FnOnce(T) -> SFut + Send + Clone + 'static,
    SFut: Future<
            Output = Result<
                (Child<E, A>, Ref),
                StartError<StartChildSpec<SFun, SFut, EFun, EFut, T, E, A, Ref>>,
            >,
        > + Send
        + 'static,
    EFun: (FnOnce(Result<E, ExitError>) -> EFut) + Send + Clone + 'static,
    EFut: Future<Output = SupervisionResult<T>> + Send + 'static,
    E: Send + 'static,
    T: Send + 'static,
    A: ActorType + Send + 'static,
    Ref: Send + 'static,
{
    type Spec = StartChildSpec<SFun, SFut, EFun, EFut, T, E, A, Ref>;

    fn poll_supervise(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<SupervisionResult<Self::Spec>> {
        let mut this = self.project();
        loop {
            if let Some(exit_fut) = this.exit_fut.as_mut().as_pin_mut() {
                break exit_fut.poll(cx).map_ok(|res| match res {
                    Some(with) => Some(StartChildSpec {
                        start_fn: this.start_fn.take().unwrap(),
                        exit_fn: this.exit_fn.take().unwrap(),
                        with,
                        phantom: PhantomData,
                    }),
                    None => None,
                });
            } else {
                let exit = ready!(this.child.poll_unpin(cx));
                let exit_fut = (this.exit_fn.as_ref().unwrap().clone())(exit);
                unsafe {
                    *this.exit_fut.as_mut().get_unchecked_mut() = Some(exit_fut);
                };
            }
        }
    }

    fn shutdown_time(self: Pin<&Self>) -> Duration {
        match self.child.link() {
            Link::Detached => get_default_shutdown_time(),
            Link::Attached(duration) => duration.to_owned(),
        }
    }

    fn halt(self: Pin<&mut Self>) {
        self.child.halt()
    }

    fn abort(self: Pin<&mut Self>) {
        self.project().child.abort();
    }
}
