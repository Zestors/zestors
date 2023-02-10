use super::*;
use async_trait::async_trait;
use futures::{ready, Future, FutureExt};
use pin_project::pin_project;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

// //------------------------------------------------------------------------------------------------
// //  Example
// //------------------------------------------------------------------------------------------------

// pub enum ChildStart<S: ChildSpecification> {
//     Success(Child<S::Exit, S::InboxType>, S::Ref),
//     Failure(S),
//     Finished,
//     Unhandled(BoxError),
// }

// pub enum ChildExit<S: ChildSpecification> {
//     RestartMe(S),
//     Finished,
//     Unhandled(BoxError),
// }

// pub struct ChildSpawnSpec<SFut, EFut, D, E, I>
// where
//     E: Send + 'static,
//     I: InboxType,
// {
//     spawn_fn: fn(I, D) -> SFut,
//     exit_fn: fn(Result<E, ProcessExitError>) -> EFut,
//     config: I::Config,
//     link: Link,
//     data: D,
//     phantom: PhantomData<(SFut, EFut, D, E, I)>,
// }

// // impl<SFut, EFut, D, E, I> ChildSpecification for ChildSpawnSpec<SFut, EFut, D, E, I>
// // where
// //     E: Send + 'static,
// //     I: InboxType,
// //     I::Config: Clone + Send,
// //     D: Send + 'static,
// //     SFut: Future<Output = E> + Send + 'static + Unpin,
// //     EFut: Future<Output = BoxResult<FnExit<D>>> + Send + Unpin,
// // {
// //     type StartFut = BoxFuture<'static, BoxResult<FnStart<D, E, I, Address<I>>>>;
// //     type ExitFut = EFut;
// //     type Data = D;
// //     type Exit = E;
// //     type InboxType = I;
// //     type Ref = Address<I>;

// //     fn start(&mut self, data: Self::Data) -> Self::StartFut {
// //         let spawn_fn = self.spawn_fn;
// //         let link = self.link.clone();
// //         let config = self.config.clone();
// //         Box::pin(async move {
// //             let (child, address) = spawn_with(link, config, move |inbox| async move {
// //                 spawn_fn(inbox, data).await
// //             });
// //             Ok(FnStart::Success(child, address))
// //         })
// //     }

// //     fn exit(&mut self, exit: Result<Self::Exit, ExitError>) -> Self::ExitFut {
// //         (self.exit_fn)(exit)
// //     }

// //     fn start_timeout(&self) -> Duration {
// //         Duration::from_millis(10)
// //     }
// // }

// #[async_trait]
// pub trait Startable {
//     type Ref;
//     type Exit: Send + 'static;
//     type InboxType: InboxType;
//     type Spec;

//     async fn start(
//         self,
//     ) -> Result<(Child<Self::Exit, Self::InboxType>, Self::Ref), StartError<Self::Spec>>;
// }

// struct StartChildSpec<D, E: Send + 'static, I: InboxType, Ref> {
//     start_fn: fn(D) -> Result<Option<(Child<E, I>, Ref)>, BoxError>,
//     exit_fn: fn(E) -> Result<Option<D>, BoxError>,
//     data: D,
// }

// struct SpawnChildSpec<D, E, I: InboxType> {
//     spawn_fn: fn(D, I) -> E,
//     exit_fn: fn(E) -> Result<Option<D>, BoxError>,
//     data: D,
//     config: I::Config,
//     link: Link,
// }



// /// Possible configurations for spawning a child:
// /// - SpawnFnSpec:
// ///     - async fn spawn(Data) -> Exit<Start>
// ///     - async fn exit()
// pub trait ChildSpecification: Sized {
//     type StartFut: Future<Output = StartResult<Self>> + Send + Unpin;
//     type ExitFut: Future<Output = ExitResult<Self>> + Send + Unpin;
//     type Exit: Send + 'static;
//     type InboxType: InboxType;
//     type Ref;

//     fn start(self) -> Self::StartFut;

//     fn exit(exit: Result<Self::Exit, ProcessExitError>) -> Self::ExitFut;

//     fn start_timeout(&self) -> Duration;
// }

// impl<S: ChildSpecification> Specification for S {
//     type Ref = S::Ref;
//     type Supervisee = ChildSupervisee<S>;
//     type Fut = ChildSpecFut<S>;

//     fn start(self) -> Self::Fut {
//         ChildSpecFut {
//             start_fut: ChildSpecification::start(self),
//         }
//     }

//     fn start_timeout(&self) -> Duration {
//         <Self as ChildSpecification>::start_timeout(&self)
//     }
// }

// #[pin_project]
// pub struct ChildSpecFut<S: ChildSpecification> {
//     start_fut: S::StartFut,
// }

// impl<S: ChildSpecification> Future for ChildSpecFut<S> {
//     type Output = StartResult<S>;

//     fn poll(
//         mut self: std::pin::Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//     ) -> std::task::Poll<Self::Output> {
//         self.start_fut.poll_unpin(cx).map(|res| match res {
//             Ok(supervisee) => todo!(),
//             Err(_) => todo!(),
//             // Start::Success((supervisee, reference)) => Start::Success((supervisee, reference)),
//             // Start::Failure(spec) => Start::Failure(spec),
//             // Start::Finished => Start::Finished,
//             // Start::Unhandled(e) => Start::Unhandled(e),
//         })
//     }
// }

// #[pin_project]
// pub struct ChildSupervisee<S: ChildSpecification> {
//     exit_fut: Option<S::ExitFut>,
//     child: Child<S::Exit, S::InboxType>,
// }

// impl<S: ChildSpecification> Supervisee for ChildSupervisee<S> {
//     type Spec = S;

//     fn abort_timeout(self: Pin<&Self>) -> Duration {
//         match self.child.link() {
//             Link::Detached => Duration::ZERO,
//             Link::Attached(duration) => duration.clone(),
//         }
//     }

//     fn halt(self: Pin<&mut Self>) {
//         self.child.halt()
//     }

//     fn abort(mut self: Pin<&mut Self>) {
//         self.child.abort();
//     }
// }

// impl<S: ChildSpecification> Future for ChildSupervisee<S> {
//     type Output = Exit<S>;

//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         loop {
//             match &mut self.exit_fut {
//                 Some(exit_fut) => match ready!(exit_fut.poll_unpin(cx)) {
//                     Exit::Finished => break Poll::Ready(Exit::Finished),
//                     Exit::Exit(spec) => break Poll::Ready(Exit::Exit(spec)),
//                     Exit::Unhandled(e) => break Poll::Ready(Exit::Unhandled(e)),
//                 },
//                 None => {
//                     let exit = ready!(self.child.poll_unpin(cx));
//                     self.exit_fut = Some(S::exit(exit));
//                 }
//             }
//         }
//     }
// }
