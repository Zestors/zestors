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
};

pub fn from_spawn_fn<I, D, SFut, E, EFut>(
    spawn_fn: impl (FnOnce(I, D) -> SFut) + Clone + Send + 'static,
    exit_fn: impl (FnOnce(Result<E, ExitError>) -> EFut) + Send + Clone + 'static,
    data: D,
    shutdown_time: Duration,
    inbox_config: I::Config,
) -> impl Specifies<Ref = Address<I>> + 'static
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFut: Future<Output = SuperviseResult<D>> + Send + 'static,
{
    SpawnSpec::new(spawn_fn, exit_fn, data, inbox_config, shutdown_time)
}

#[pin_project]
pub(crate) struct SpawnSpec<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(I, D) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = SuperviseResult<D>> + Send,
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
    SFun: FnOnce(I, D) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = SuperviseResult<D>> + Send,
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
}

impl<SFun, SFut, EFun, EFut, D, E, I> Specifies for SpawnSpec<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(I, D) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ExitError>) -> EFut + Send + Clone + 'static,
    EFut: Future<Output = SuperviseResult<D>> + Send + 'static,
{
    type Ref = Address<I>;
    type Supervisee = SpawnSupervisee<SFun, SFut, EFun, EFut, D, E, I>;
    type Fut = Ready<StartResult<Self>>;

    fn start(self) -> Self::Fut {
        let inner = self.inner.clone();
        let (child, address) = spawn_with(
            Link::Attached(inner.abort_timeout),
            inner.config,
            move |inbox| async move {
                let spawn_fut = (inner.spawn_fn)(inbox, self.data);
                spawn_fut.await
            },
        );
        let res = Ok((
            SpawnSupervisee {
                inner: Some(self.inner),
                child,
                exit_fut: None,
            },
            address,
        ));
        future::ready(res)
    }
}

#[pin_project]
pub(crate) struct SpawnSupervisee<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(I, D) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = SuperviseResult<D>> + Send,
{
    inner: Option<Inner<SFun, SFut, EFun, EFut, D, E, I>>,
    child: Child<E, I>,
    #[pin]
    exit_fut: Option<EFut>,
}

impl<SFun, SFut, EFun, EFut, D, E, I> Supervisable
    for SpawnSupervisee<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(I, D) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ExitError>) -> EFut + Send + Clone + 'static,
    EFut: Future<Output = SuperviseResult<D>> + Send + 'static,
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

    fn poll_supervise(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<SuperviseResult<Self::Spec>> {
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
        let _x = SpawnSpec::new(
            |_inbox: Halter, _data: u32| async move { () },
            |exit| async move {
                match exit {
                    Ok(_res) => todo!(),
                    Err(e) => match e {
                        ExitError::Panic(_) => todo!(),
                        ExitError::Abort => todo!(),
                    },
                }
            },
            10,
            Default::default(),
            get_default_shutdown_time(),
        );
    }

    fn spec() -> impl Specifies<Ref = Address<Halter>> {
        SpawnSpec::new(
            |_inbox: Halter, _data: u32| async move { () },
            |exit| async move {
                match exit {
                    Ok(_res) => todo!(),
                    Err(e) => match e {
                        ExitError::Panic(_) => todo!(),
                        ExitError::Abort => todo!(),
                    },
                }
            },
            10,
            Default::default(),
            get_default_shutdown_time(),
        )
    }
}

struct Inner<SFun, SFut, EFun, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Send + Clone,
    D: Send + 'static,
    SFun: FnOnce(I, D) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = SuperviseResult<D>> + Send,
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
    SFun: FnOnce(I, D) -> SFut + Send + Clone + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFun: FnOnce(Result<E, ExitError>) -> EFut + Send + Clone,
    EFut: Future<Output = SuperviseResult<D>> + Send,
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
