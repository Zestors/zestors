use super::*;
use futures::{
    future::{self, Ready},
    ready, Future, FutureExt,
};
use pin_project::pin_project;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

#[pin_project]
pub struct SpawnSpec<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(D, I) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ProcessExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = ExitResult<D>> + Send,
{
    inner: Inner<SFun, SFut, EFun, EFut, D, E, I>,
    data: D,
}

impl<SFun, SFut, EFun, EFut, D, E, I> SpawnSpec<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(D, I) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ProcessExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = ExitResult<D>> + Send,
{
    pub fn new(
        spawn_fn: SFun,
        exit_fn: EFun,
        data: D,
        config: I::Config,
        abort_timeout: Duration,
    ) -> Self {
        Self {
            inner: Inner {
                spawn_fn,
                exit_fn,
                config,
                abort_timeout,
                phantom: PhantomData,
            },
            data,
        }
    }

    pub fn new_default(spawn_fn: SFun, exit_fn: EFun, data: D) -> Self
    where
        I::Config: Default,
    {
        Self::new(
            spawn_fn,
            exit_fn,
            data,
            Default::default(),
            Duration::from_secs(1),
        )
    }
}

impl<SFun, SFut, EFun, EFut, D, E, I> Specification for SpawnSpec<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(D, I) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ProcessExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = ExitResult<D>> + Send,
{
    type Ref = Address<I>;
    type Supervisee = SpawnSupervisee<SFun, SFut, EFun, EFut, D, E, I>;
    type StartFut = Ready<StartResult<Self>>;

    fn start(self) -> Self::StartFut {
        future::ready({
            let inner = self.inner.clone();
            let (child, address) = spawn_with(
                Link::Attached(inner.abort_timeout),
                inner.config,
                move |inbox| async move {
                    let spawn_fut = (inner.spawn_fn)(self.data, inbox);
                    spawn_fut.await
                },
            );

            Ok((
                SpawnSupervisee {
                    inner: Some(self.inner),
                    child,
                    exit_fut: None,
                },
                address,
            ))
        })
    }

    fn start_time(&self) -> Duration {
        Duration::from_millis(10)
    }
}

#[pin_project]
pub struct SpawnSupervisee<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(D, I) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ProcessExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = ExitResult<D>> + Send,
{
    inner: Option<Inner<SFun, SFut, EFun, EFut, D, E, I>>,
    child: Child<E, I>,
    #[pin]
    exit_fut: Option<EFut>,
}

impl<SFun, SFut, EFun, EFut, D, E, I> Supervisee
    for SpawnSupervisee<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(D, I) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ProcessExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = ExitResult<D>> + Send,
{
    type Spec = SpawnSpec<SFun, SFut, EFun, EFut, D, E, I>;

    fn shutdown_time(self: Pin<&Self>) -> Duration {
        match self.child.link() {
            Link::Detached => panic!(),
            Link::Attached(duration) => duration.clone(),
        }
    }

    fn halt(self: Pin<&mut Self>) {
        self.child.halt();
    }

    fn abort(self: Pin<&mut Self>) {
        self.project().child.abort();
    }
}

impl<SFun, SFut, EFun, EFut, D, E, I> Future for SpawnSupervisee<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(D, I) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ProcessExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = ExitResult<D>> + Send,
{
    type Output = ExitResult<<Self as Supervisee>::Spec>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.as_mut().project();

        loop {
            match this.exit_fut.as_mut().as_pin_mut() {
                Some(exit_fut) => {
                    break exit_fut.poll(cx).map(|ready| match ready {
                        Ok(Some(data)) => Ok(Some(SpawnSpec {
                            inner: this.inner.take().unwrap(),
                            data,
                        })),
                        Ok(None) => Ok(None),
                        Err(e) => Err(e),
                    });
                }
                None => {
                    let exit = ready!(this.child.poll_unpin(cx));
                    let exit_fut = (this.inner.take().unwrap().exit_fn)(exit);
                    unsafe {
                        *this.exit_fut.as_mut().get_unchecked_mut() = Some(exit_fut);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test() {
        let x = SpawnSpec::new(
            |data: u32, inbox: Halter| async move { () },
            |exit| async move {
                match exit {
                    Ok(res) => todo!(),
                    Err(e) => match e {
                        ProcessExitError::Panic(_) => todo!(),
                        ProcessExitError::Abort => todo!(),
                    },
                }
            },
            10,
            Default::default(),
            Duration::from_secs(1),
        );
    }

    fn spec() -> impl Specification<Ref = Address<Halter>> + Send + Unpin {
        SpawnSpec::new(
            |data: u32, inbox: Halter| async move { () },
            |exit| async move {
                match exit {
                    Ok(res) => todo!(),
                    Err(e) => match e {
                        ProcessExitError::Panic(_) => todo!(),
                        ProcessExitError::Abort => todo!(),
                    },
                }
            },
            10,
            Default::default(),
            Duration::from_secs(1),
        )
    }
}

struct Inner<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(D, I) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ProcessExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = ExitResult<D>> + Send,
{
    spawn_fn: SFun,
    exit_fn: EFun,
    config: I::Config,
    abort_timeout: Duration,
    phantom: PhantomData<(SFut, EFut)>,
}

impl<SFun, SFut, EFun, EFut, D, E, I> Clone for Inner<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(D, I) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ProcessExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = ExitResult<D>> + Send,
{
    fn clone(&self) -> Self {
        Self {
            spawn_fn: self.spawn_fn.clone(),
            exit_fn: self.exit_fn.clone(),
            config: self.config.clone(),
            abort_timeout: self.abort_timeout.clone(),
            phantom: self.phantom,
        }
    }
}
