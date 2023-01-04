// use crate::*;
// use futures::FutureExt;
// use std::{
//     mem,
//     pin::Pin,
//     task::{Context, Poll},
// };
// use tiny_actor::ExitError;

// //------------------------------------------------------------------------------------------------
// //  SupervisableChild
// //------------------------------------------------------------------------------------------------

// pub struct SupervisableChild<E, P, S>
// where
//     E: Send + 'static,
//     P: Protocol,
//     S: SpecifiesChild,
// {
//     child: Child<E, P>,
//     startable: S,
//     state: ChildState<E>,
// }

// impl<E, P, S> SupervisableChild<E, P, S>
// where
//     E: Send + 'static,
//     P: Protocol,
//     S: SpecifiesChild + Send + 'static,
// {
//     pub fn new(child: Child<E, P>, startable: S) -> Self {
//         Self {
//             child,
//             startable,
//             state: ChildState::Alive,
//         }
//     }

//     pub fn into_dyn(self) -> DynamicallySupervisableChild {
//         DynamicallySupervisableChild(Box::new(self))
//     }

//     pub fn supervise(&mut self) -> SuperviseFut<Self> {
//         SuperviseFut(self)
//     }

//     pub fn to_restart(&mut self) -> ToRestartFut<Self> {
//         ToRestartFut(self)
//     }

//     pub fn restart(&mut self) -> RestartFut<Self> {
//         RestartFut(self)
//     }
// }

// impl<E, P, S> Unpin for SupervisableChild<E, P, S>
// where
//     E: Send + 'static,
//     P: Protocol,
//     S: SpecifiesChild<Exit = E, Protocol = P>,
// {
// }

// impl<E, P, S> PollSupervisable for SupervisableChild<E, P, S>
// where
//     E: Send + 'static,
//     P: Protocol,
//     S: SpecifiesChild<Exit = E, Protocol = P>,
// {
//     fn poll_supervise(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
//         let ChildState::Alive = &self.state else {
//             panic!("Child must be alive to supervise")
//         };

//         self.child.poll_unpin(cx).map(|res| {
//             self.state = ChildState::Exited(res);
//             ()
//         })
//     }

//     fn poll_to_restart(
//         mut self: Pin<&mut Self>,
//         cx: &mut Context<'_>,
//     ) -> Poll<Result<bool, StartError>> {
//         let res = {
//             let mut state = ChildState::PollingRestart;
//             mem::swap(&mut self.state, &mut state);
//             let ChildState::Exited(res) = state else {
//                 panic!("Child must be alive to supervise")
//             };
//             res
//         };

//         Pin::new(&mut self.startable)
//             .poll_to_restart(res, cx)
//             .map(|res| match res {
//                 Ok(true) => {
//                     self.state = ChildState::ReadyForRestart;
//                     res
//                 }
//                 Ok(false) => {
//                     self.state = ChildState::Finished;
//                     res
//                 }
//                 Err(_) => {
//                     self.state = ChildState::StartFailed;
//                     res
//                 }
//             })
//     }

//     fn poll_restart(
//         mut self: Pin<&mut Self>,
//         cx: &mut Context<'_>,
//     ) -> Poll<Result<(), StartError>> {
//         let ChildState::ReadyForRestart = &self.state else {
//             panic!("Child must be alive to supervise")
//         };
//         let link = self.child.link().clone();
//         Pin::new(&mut self.startable)
//             .poll_start(link, cx)
//             .map(|res| match res {
//                 Ok(mut child) => {
//                     mem::swap(&mut self.child, &mut child);
//                     self.state = ChildState::Alive;
//                     Ok(())
//                 }
//                 Err(e) => {
//                     self.state = ChildState::StartFailed;
//                     Err(e)
//                 }
//             })
//     }
// }

// //------------------------------------------------------------------------------------------------
// //  BoxedSupervisableChild
// //------------------------------------------------------------------------------------------------

// pub struct DynamicallySupervisableChild(Box<dyn PollSupervisable + Send>);

// impl DynamicallySupervisableChild {
//     pub fn supervise(&mut self) -> SuperviseFut {
//         SuperviseFut(&mut self.0)
//     }

//     pub fn to_restart(&mut self) -> ToRestartFut {
//         self.0.to_restart()
//     }

//     pub fn restart(&mut self) -> RestartFut {
//         self.0.restart()
//     }
// }

// impl DynamicallySupervisable for DynamicallySupervisableChild {
//     fn supervise(&mut self) -> SuperviseFut {
//         self.0.supervise()
//     }

//     fn to_restart(&mut self) -> ToRestartFut {
//         self.0.to_restart()
//     }

//     fn restart(&mut self) -> RestartFut {
//         self.0.restart()
//     }
// }

// //------------------------------------------------------------------------------------------------
// //  ChildState
// //------------------------------------------------------------------------------------------------

// #[derive(Debug)]
// enum ChildState<E> {
//     Alive,
//     Exited(Result<E, ExitError>),
//     Finished,
//     PollingRestart,
//     ReadyForRestart,
//     StartFailed,
// }
