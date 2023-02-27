use crate::{
    actor_type::{ActorType, ActorInbox},
    actor_ref::{Address, ActorRefExt, Child, ExitError},
    supervision::{
        combinator_specs::{spawn, Link, ShutdownTime},
        StartError, StartResult, Specifies, Supervisable, SuperviseResult,
    },
};
use futures::{future::BoxFuture, Future, FutureExt};
use pin_project::pin_project;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{ready, Context, Poll},
    time::Duration,
};

fn from_spawn_fn<D, I, SFut, E, EFut>(
    spawn_fn: impl (FnOnce(I, D) -> SFut) + Clone + 'static,
    exit_fn: impl (FnOnce(Result<E, ExitError>) -> EFut) + Send + Clone,
    start_time: Duration,
    data: D,
) where
    E: Send + 'static,
    D: Send + 'static,
    I: ActorInbox,
    I::Config: Default,
    SFut: Future<Output = E> + Send + 'static,
    EFut: Future<Output = SuperviseResult<D>> + Send + 'static,
{
    // let spec = ChildSpec::new(
    //     |data: D, start_time: Duration| async move {
    //         spawn(|inbox: I| async move { spawn_fn(inbox, data).await })
    //     },
    //     |exit_result: Result<E, ExitError>| async move {
    //         match exit_fn(exit_result).await {
    //             Ok(Some(data)) => Ok(Some((data, start_time))),
    //             Ok(None) => Ok(None),
    //             Err(e) => Err(e),
    //         }
    //     },
    //     start_time,
    //     data,
    // );
}

// expected that resolves to   `Result<Child<_, _>,  _),          StartError<ChildSpec<_, [async block@zestors/src/supervision/child_spec/start.rs:32:41: 39:10], _, _, D, _, _, _>>>`,
// but it resolves to          `Result<(Child<E, I>, Address<I>), _>`

// expected that resolves to   `Result<(monitoring::child::Child<_, _>, _), startable::StartError<start::ChildSpec<_, [async block@zestors/src/supervision/child_spec/start.rs:32:41: 40:10], _, _, D, _, _, _>>>`,
// but it resolves to          `Result<monitoring::child::Child<E, I>, _>`

#[pin_project]
pub struct ChildSpec<SFun, SFut, EFun, EFut, D, E, A, Ref> {
    start_fn: SFun,
    exit_fn: EFun,
    start_time: Duration,
    data: D,
    phantom: PhantomData<(SFut, EFut, E, A, Ref)>,
}

impl<SFun, SFut, EFun, EFut, D, E, A, Ref> ChildSpec<SFun, SFut, EFun, EFut, D, E, A, Ref>
where
    SFun: (FnOnce(D, Duration) -> SFut) + Send + Clone + 'static,
    SFut: Future<Output = Result<(Child<E, A>, Ref), StartError<Self>>> + Send + 'static,
    SFut: Send + 'static + Future,
    EFun: (FnOnce(Result<E, ExitError>) -> EFut) + Send + Clone + 'static,
    EFut: Future<Output = SuperviseResult<(D, Duration)>> + Send + 'static,
    E: Send + 'static,
    D: Send + 'static,
    A: ActorType + Send + 'static,
    Ref: Send + 'static,
{
    pub fn new(start_fn: SFun, exit_fn: EFun, start_time: Duration, data: D) -> Self {
        Self {
            start_fn,
            exit_fn,
            start_time,
            data,
            phantom: PhantomData,
        }
    }
}

impl<SFun, SFut, EFun, EFut, D, E, A, Ref> Specifies
    for ChildSpec<SFun, SFut, EFun, EFut, D, E, A, Ref>
where
    SFun: (FnOnce(D, Duration) -> SFut) + Send + Clone + 'static,
    SFut: Future<
            Output = Result<
                (Child<E, A>, Ref),
                StartError<ChildSpec<SFun, SFut, EFun, EFut, D, E, A, Ref>>,
            >,
        > + Send
        + 'static,
    SFut: Send + 'static + Future,
    EFun: (FnOnce(Result<E, ExitError>) -> EFut) + Send + Clone + 'static,
    EFut: Future<Output = SuperviseResult<(D, Duration)>> + Send + 'static,
    E: Send + 'static,
    D: Send + 'static,
    A: ActorType + Send + 'static,
    Ref: Send + 'static,
{
    type Ref = Ref;
    type Supervisee = ChildSupervisee<SFun, SFut, EFun, EFut, D, E, A, Ref>;
    type Fut = BoxFuture<'static, StartResult<Self>>;

    fn start(self) -> Self::Fut {
        Box::pin(async move {
            let start_fut = (self.start_fn.clone())(self.data, self.start_time);
            match start_fut.await {
                Ok((child, reference)) => {
                    let supervisee = ChildSupervisee {
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
        })
    }

}

#[pin_project]
pub struct ChildSupervisee<SFun, SFut, EFun, EFut, D, E, A, Ref>
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

impl<SFun, SFut, EFun, EFut, D, E, A, Ref> Supervisable
    for ChildSupervisee<SFun, SFut, EFun, EFut, D, E, A, Ref>
where
    SFun: (FnOnce(D, Duration) -> SFut) + Send + Clone + 'static,
    SFut: Future<
            Output = Result<
                (Child<E, A>, Ref),
                StartError<ChildSpec<SFun, SFut, EFun, EFut, D, E, A, Ref>>,
            >,
        > + Send
        + 'static,
    EFun: (FnOnce(Result<E, ExitError>) -> EFut) + Send + Clone + 'static,
    EFut: Future<Output = SuperviseResult<(D, Duration)>> + Send + 'static,
    E: Send + 'static,
    D: Send + 'static,
    A: ActorType + Send + 'static,
    Ref: Send + 'static,
{
    type Spec = ChildSpec<SFun, SFut, EFun, EFut, D, E, A, Ref>;

    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context) -> Poll<SuperviseResult<Self::Spec>> {
        let mut this = self.project();
        loop {
            if let Some(exit_fut) = this.exit_fut.as_mut().as_pin_mut() {
                break exit_fut.poll(cx).map_ok(|res| match res {
                    Some((data, start_time)) => Some(ChildSpec {
                        start_fn: this.start_fn.take().unwrap(),
                        exit_fn: this.exit_fn.take().unwrap(),
                        start_time,
                        data,
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

    fn shutdown_time(self: Pin<&Self>) -> ShutdownTime {
        match self.child.link() {
            Link::Detached => ShutdownTime::Default,
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
