use super::*;
use async_trait::async_trait;
use futures::{future::BoxFuture, ready, Future, FutureExt};
use pin_project::pin_project;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

//------------------------------------------------------------------------------------------------
//  ChildSpecification
//------------------------------------------------------------------------------------------------

pub trait ChildSpecification: Sized {
    type StartFut: Future<
        Output = Result<(Child<Self::Exit, Self::ActorType>, Self::Ref), StartError<Self>>,
    >;
    type ExitFut: Future<Output = ExitResult<Self>>;
    type Exit: Send + 'static;
    type ActorType: ActorType;
    type Ref;

    fn start(self) -> Self::StartFut;

    fn exit(exit: Result<Self::Exit, ProcessExitError>) -> Self::ExitFut;

    fn start_timeout(&self) -> Duration;
}

pub struct ChildSpec<S: ChildSpecification> {
    spec: S,
}

impl<S: ChildSpecification> Specification for ChildSpec<S> {
    type Ref = S::Ref;
    type Supervisee = ChildSupervisee<S>;
    type StartFut = ChildSpecFut<S>;

    fn start(self) -> Self::StartFut {
        ChildSpecFut {
            fut: <S as ChildSpecification>::start(self.spec),
        }
    }

    fn start_time(&self) -> Duration {
        <S as ChildSpecification>::start_timeout(&self.spec)
    }
}

#[pin_project]
pub struct ChildSpecFut<S: ChildSpecification> {
    #[pin]
    fut: S::StartFut,
}

impl<S: ChildSpecification> Future for ChildSpecFut<S> {
    type Output = StartResult<ChildSpec<S>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().fut.poll(cx).map(|ready| match ready {
            Ok((child, reference)) => Ok((ChildSupervisee::new(child), reference)),
            Err(StartError::Completed) => Err(StartError::Completed),
            Err(StartError::Irrecoverable(e)) => Err(StartError::Irrecoverable(e)),
            Err(StartError::Failed(spec)) => Err(StartError::Failed(ChildSpec { spec })),
        })
    }
}

#[pin_project]
pub struct ChildSupervisee<S: ChildSpecification> {
    #[pin]
    exit_fut: Option<S::ExitFut>,
    child: Child<S::Exit, S::ActorType>,
}

impl<S: ChildSpecification> ChildSupervisee<S> {
    pub fn new(child: Child<S::Exit, S::ActorType>) -> Self {
        Self {
            exit_fut: None,
            child,
        }
    }
}

impl<S: ChildSpecification> Supervisee for ChildSupervisee<S> {
    type Spec = ChildSpec<S>;

    fn shutdown_time(self: Pin<&Self>) -> Duration {
        match self.child.link() {
            Link::Detached => Duration::from_secs(1),
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

impl<S: ChildSpecification> Future for ChildSupervisee<S> {
    type Output = ExitResult<ChildSpec<S>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            match this.exit_fut.as_mut().as_pin_mut() {
                Some(exit_fut) => {
                    break exit_fut.poll(cx).map(|ready| match ready {
                        Ok(Some(spec)) => Ok(Some(ChildSpec { spec })),
                        Ok(None) => Ok(None),
                        Err(e) => Err(e),
                    });
                }
                None => {
                    let exit = ready!(this.child.poll_unpin(cx));
                    unsafe {
                        // Value set is pinned
                        *this.exit_fut.as_mut().get_unchecked_mut() = Some(S::exit(exit));
                    }
                }
            }
        }
    }
}

impl<S: ChildSpecification> ChannelRef for ChildSupervisee<S> {
    type ActorType = S::ActorType;

    fn channel(&self) -> &std::sync::Arc<<Self::ActorType as ActorType>::Channel> {
        self.child.channel()
    }
}

//------------------------------------------------------------------------------------------------
//  DynChildSpecification
//------------------------------------------------------------------------------------------------

#[async_trait]
pub trait DynChildSpecification: 'static + Sized + Send {
    type Exit: Send + 'static;
    type ActorType: ActorType;
    type Ref;

    async fn start(
        self,
    ) -> Result<(Child<Self::Exit, Self::ActorType>, Self::Ref), StartError<Self>>;

    async fn exit(exit: Result<Self::Exit, ProcessExitError>) -> Result<Option<Self>, BoxError>;

    fn start_timeout(&self) -> Duration;
}

impl<S: DynChildSpecification> ChildSpecification for S {
    type StartFut = BoxFuture<
        'static,
        Result<(Child<Self::Exit, Self::ActorType>, Self::Ref), StartError<Self>>,
    >;
    type ExitFut = BoxFuture<'static, Result<Option<Self>, BoxError>>;
    type Exit = S::Exit;
    type ActorType = S::ActorType;
    type Ref = S::Ref;

    fn start(self) -> Self::StartFut {
        <S as DynChildSpecification>::start(self)
    }

    fn exit(exit: Result<Self::Exit, ProcessExitError>) -> Self::ExitFut {
        <S as DynChildSpecification>::exit(exit)
    }

    fn start_timeout(&self) -> Duration {
        <S as DynChildSpecification>::start_timeout(self)
    }
}

//------------------------------------------------------------------------------------------------
//  Examples
//------------------------------------------------------------------------------------------------

pub struct ChildSpawnSpec<SFut, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
{
    spawn_fn: fn(I, D) -> SFut,
    exit_fn: fn(Result<E, ProcessExitError>) -> EFut,
    config: I::Config,
    link: Link,
    data: D,
    phantom: PhantomData<(SFut, EFut, D, E, I)>,
}

#[async_trait]
impl<SFut, EFut, D, E, I> DynChildSpecification for ChildSpawnSpec<SFut, EFut, D, E, I>
where
    E: Send + 'static,
    I: InboxType,
    I::Config: Clone + Send,
    D: Send + 'static,
    SFut: Future<Output = E> + Send + 'static,
    EFut: Future<Output = ExitResult<D>> + Send + 'static,
{
    type Exit = E;
    type ActorType = I;
    type Ref = Address<I>;

    async fn start(
        self,
    ) -> Result<(Child<Self::Exit, Self::ActorType>, Self::Ref), StartError<Self>> {
        todo!()
    }

    async fn exit(exit: Result<Self::Exit, ProcessExitError>) -> Result<Option<Self>, BoxError> {
        todo!()
    }

    fn start_timeout(&self) -> Duration {
        Duration::from_millis(10)
    }

//     fn into_spec(self) -> ChildSpec<ChildSpawnSpec<SFut, EFut, D, E, I>> {
//         ChildSpec { spec: self }
//     }
}



// pub fn spec() -> impl Specification<Ref = Address<Halter>> {
//     ChildSpawnSpec {
//         spawn_fn: |halter: Halter, _| async move { halter.await },
//         exit_fn: |_| async move { Ok(Some(())) },
//         config: (),
//         link: Link::Detached,
//         data: (),
//         phantom: PhantomData,
//     }
// }

// pub fn test() -> impl Specification<Ref = ()> {
//     spec().on_start(move |address| {
//         address.halt();
//         ()
//     })
// }
