// use std::{
//     pin::Pin,
//     task::{Context, Poll},
// };

// use futures::{ready, Future, FutureExt};

// use crate::*;

// //------------------------------------------------------------------------------------------------
// //  StartableExt
// //------------------------------------------------------------------------------------------------

// pub trait StartableExt: Startable + Sized + Unpin {
//     fn start(self) -> StartFut<Self> {
//         StartFut(Some(self))
//     }
// }

// impl<T> StartableExt for T where T: Startable + Unpin {}

// //-------------------------------------------------
// //  StartFut
// //-------------------------------------------------

// pub struct StartFut<S: Startable + Unpin>(Option<S>);

// impl<S: Startable + Unpin> Unpin for StartFut<S> {}

// impl<S: Startable + Unpin> Future for StartFut<S> {
//     type Output = Result<(Child<S::Exit, S::ActorType>, S::Ref), StartError>;

//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         Pin::new(self.0.as_mut().unwrap()).poll_start(cx)
//     }
// }

// //------------------------------------------------------------------------------------------------
// //  SpecifiesChildExt
// //------------------------------------------------------------------------------------------------

// pub trait SpecifiesChildExt: Supervisable + Sized + Unpin {
//     fn start_supervised(self) -> StartSupervisedFut<Self> {
//         StartSupervisedFut(Some(self))
//     }
// }

// impl<T> SpecifiesChildExt for T where T: Supervisable + Unpin {}

// //-------------------------------------------------
// //  SupervisedStartFut
// //-------------------------------------------------

// pub struct StartSupervisedFut<S: Supervisable + Unpin>(Option<S>);

// impl<S: Supervisable + Unpin> Unpin for StartSupervisedFut<S> {}

// impl<S: Supervisable + Unpin> Future for StartSupervisedFut<S> {
//     type Output = Result<(SupervisedChild<S>, S::Ref), StartError>;

//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         Pin::new(self.0.as_mut().unwrap())
//             .poll_start(cx)
//             .map_ok(|(child, reference)| {
//                 (
//                     SupervisedChild {
//                         specifier: self.0.take().unwrap(),
//                         child,
//                         state: SupervisedChildState::ChildExit,
//                     },
//                     reference,
//                 )
//             })
//     }
// }

// //------------------------------------------------------------------------------------------------
// //  SupervisedChild
// //------------------------------------------------------------------------------------------------

// pub struct SupervisedChild<S: Supervisable> {
//     state: SupervisedChildState,
//     specifier: S,
//     child: Child<S::Exit, S::ActorType>,
// }

// pub enum SupervisedChildState {
//     ChildExit,
//     Finished,
//     Restart,
//     RestartFailure,
//     ShouldRestart,
// }

// impl<S: Supervisable> SupervisedChild<S> {
//     pub fn supervise(&mut self) -> SupervisionFut<'_, S> {
//         SupervisionFut(self)
//     }
// }

// //------------------------------------------------------------------------------------------------
// //  SupervisionFut
// //------------------------------------------------------------------------------------------------

// pub struct SupervisionFut<'a, S: Supervisable>(&'a mut SupervisedChild<S>);

// impl<'a, S: Supervisable> Unpin for SupervisionFut<'a, S> {}

// impl<'a, S: Supervisable + Unpin> Future for SupervisionFut<'a, S> {
//     type Output = Result<SupervisionItem<S>, StartError>;

//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         loop {
//             match &self.0.state {
//                 SupervisedChildState::ChildExit => {
//                     let exit_result = ready!(self.0.child.poll_unpin(cx));
//                     Pin::new(&mut self.0.specifier).init_poll_after_exit(exit_result);
//                     self.0.state = SupervisedChildState::ShouldRestart;
//                 }
//                 SupervisedChildState::ShouldRestart => {
//                     let should_restart =
//                         ready!(Pin::new(&mut self.0.specifier).poll_after_exit(cx));
//                     let output = match should_restart {
//                         Ok(should_restart) => {
//                             if should_restart {
//                                 self.0.state = SupervisedChildState::Restart;
//                                 Ok(SupervisionItem::Exited { restart: true })
//                             } else {
//                                 self.0.state = SupervisedChildState::Finished;
//                                 Ok(SupervisionItem::Exited { restart: false })
//                             }
//                         }
//                         Err(e) => {
//                             self.0.state = SupervisedChildState::RestartFailure;
//                             Err(e)
//                         }
//                     };
//                     break Poll::Ready(output);
//                 }
//                 SupervisedChildState::Restart => {
//                     let restart_result = ready!(Pin::new(&mut self.0.specifier).poll_start(cx));
//                     let output = match restart_result {
//                         Ok((child, reference)) => {
//                             self.0.child = child;
//                             self.0.state = SupervisedChildState::ChildExit;
//                             Ok(SupervisionItem::Restarted(reference))
//                         }
//                         Err(e) => {
//                             self.0.state = SupervisedChildState::RestartFailure;
//                             Err(e)
//                         }
//                     };
//                     break Poll::Ready(output);
//                 }
//                 SupervisedChildState::Finished => panic!("Can't restart process that is finished"),
//                 SupervisedChildState::RestartFailure => {
//                     todo!("Can't restart process that has failed to start")
//                 }
//             }
//         }
//     }
// }

// pub enum SupervisionItem<S: Supervisable> {
//     /// The actor has exited. `restart` says whether the actor wants to be restarted.
//     Exited { restart: bool },
//     /// The actor has been successfully restarted. Returned is the new reference to
//     /// the actor.
//     Restarted(S::Ref),
// }

// //------------------------------------------------------------------------------------------------
// //  SimpleChildSpec
// //------------------------------------------------------------------------------------------------

// pub struct SimpleChildSpec<P, E, I, SFut, RFut>
// where
//     P: Protocol,
//     E: Send + 'static,
//     SFut: Future<Output = E> + Send + 'static,
//     RFut: Future<Output = Result<Option<I>, StartError>> + Send + 'static,
//     I: Send + 'static,
// {
//     spawn_fn: fn(Inbox<P>, I) -> SFut,
//     restart_fn: fn(Result<E, ExitError>) -> RFut,
//     config: Config,
//     init: Option<I>,
//     restart_fut: Option<Pin<Box<RFut>>>,
// }

// impl<P, E, I, SFut, RFut> Unpin for SimpleChildSpec<P, E, I, SFut, RFut>
// where
//     P: Protocol,
//     E: Send + 'static,
//     SFut: Future<Output = E> + Send + 'static,
//     RFut: Future<Output = Result<Option<I>, StartError>> + Send + 'static,
//     I: Send + 'static,
// {
// }

// impl<P, E, I, SFut, RFut> Startable for SimpleChildSpec<P, E, I, SFut, RFut>
// where
//     P: Protocol,
//     E: Send + 'static,
//     SFut: Future<Output = E> + Send + 'static,
//     RFut: Future<Output = Result<Option<I>, StartError>> + Send + 'static,
//     I: Send + 'static,
// {
//     type Exit = E;
//     type ActorType = P;
//     type Ref = Address<P>;

//     fn poll_start(
//         mut self: Pin<&mut Self>,
//         cx: &mut Context,
//     ) -> Poll<Result<(Child<Self::Exit, Self::ActorType>, Self::Ref), StartError>> {
//         let spawn_fun = self.spawn_fn;
//         let config = self.config.clone();
//         let init = self.init.take().unwrap();
//         let (child, address) =
//             spawn_process(
//                 config,
//                 move |inbox| async move { spawn_fun(inbox, init).await },
//             );
//         Poll::Ready(Ok((child, address)))
//     }
// }

// impl<P, E, I, SFut, RFut> Supervisable for SimpleChildSpec<P, E, I, SFut, RFut>
// where
//     P: Protocol,
//     E: Send + 'static,
//     SFut: Future<Output = E> + Send + 'static,
//     RFut: Future<Output = Result<Option<I>, StartError>> + Send + 'static,
//     I: Send + 'static,
// {
//     fn init_poll_after_exit(mut self: Pin<&mut Self>, exit: Result<Self::Exit, ExitError>) {
//         self.restart_fut = Some(Box::pin((self.restart_fn)(exit)));
//     }

//     // fn poll_requires_restart(
//     //     mut self: Pin<&mut Self>,
//     //     cx: &mut Context,
//     // ) -> Poll<Result<bool, StartError>> {
//     //     let should_restart =
//     //         ready!(Pin::new(&mut self.restart_fut.as_mut().unwrap()).poll_unpin(cx));
//     //     self.restart_fut = None;
//     //     Poll::Ready(should_restart.map(|init| match init {
//     //         Some(init) => {
//     //             self.init = Some(init);
//     //             true
//     //         }
//     //         None => false,
//     //     }))
//     // }

//     fn poll_restart(
//         self: Pin<&mut Self>,
//         cx: &mut Context,
//     ) -> Poll<Result<Option<(Child<Self::Exit, Self::ActorType>, Self::Ref)>, StartError>> {
//         self.poll_start(cx).map_ok(|some| Some(some))
//     }
// }
