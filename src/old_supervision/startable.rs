use crate::*;
use async_trait::async_trait;
use futures::{Future, FutureExt, Stream};
use std::{
    error::Error,
    marker::PhantomData,
    mem,
    pin::Pin,
    task::{Context, Poll},
};
use tiny_actor::{ExitError, Link};

use super::child_spec;

//------------------------------------------------------------------------------------------------
//  Startable
//------------------------------------------------------------------------------------------------

pub trait SpecifiesChild: Unpin {
    /// After the process has exited, it will be polled whether it wants to be restarted.
    /// This should return `Ok(true)` if it wants to be restarted, and `Ok(false)` if the
    /// process is finished.
    ///
    /// If the process is not finished, but can't restart because of a problem, then an
    /// `Err(StartError)` may be returned. This will cause the supervisor itself to exit
    /// and restart.
    ///
    /// # Panics
    /// This function is allowed to panic if:
    /// - The process is still alive.
    /// - `poll_start` has not been called before this function.
    fn poll_to_restart(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<bool, StartError>>;

    /// This is polled whenever the child needs to be started. This can either for the first
    /// start, or after `poll_restart` is called.
    ///
    /// # Panics
    /// This function is allowed to panic if:
    /// - The process is still alive.
    /// - `poll_restart` returned something other than `Ok(true)` before calling this.
    /// - `poll_restart` was not called before attempting a restart.
    fn poll_start(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), StartError>>;
}

#[async_trait]
pub trait SpecifiesChildV2 {
    type Supervisable: PrivSupervisableV2;

    async fn start(self, link: Link) -> Result<Self::Supervisable, StartError>;
}

#[async_trait]
pub trait PrivSupervisableV2 {
    async fn to_restart(&mut self) -> Result<bool, StartError>;
    async fn restart(&mut self) -> Result<(), StartError>;
}

#[async_trait]
pub trait Supervisable {
    type Exit: Send + 'static;
    type Protocol: Protocol;

    async fn to_restart(&mut self) -> Result<bool, StartError>;
    async fn restart(&mut self) -> Result<Child<Self::Exit, Self::Protocol>, StartError>;
}

pub struct BasicSupervisedChildV2<E, P>
where
    E: Send + 'static,
    P: Protocol,
{
    child: Child<E, P>,
    state: (),
}

pub struct BasicChildSpec<E, I, P> {
    restart_fn: fn(Result<E, ExitError>) -> bool,
    start_fn: fn(I, Inbox<P>) -> E,
    init: I,
}

// #[async_trait]
// impl<E, I, P> SpecifiesChildV2 for BasicChildSpec<E, I, P>
// where
//     E: Send + 'static,
//     I: Send,
//     P: Protocol
// {
//     type Supervisable = ();

//     async fn start(&mut self) -> Result<(), StartError> {
//         todo!()
//     }
// }

// //------------------------------------------------------------------------------------------------
// //  V3
// //------------------------------------------------------------------------------------------------

// pub trait SpecifiesChildV3 {
//     type Exit: Send + 'static;
//     type Protocol: Protocol;

//     type StartFut<'a>: Future<Output = Result<(), StartError>> + Unpin + Send + 'a
//     where
//         Self: 'a;

//     fn start(&mut self) -> Self::StartFut<'_>;

//     type ToRestartFut<'a>: Future<Output = Result<bool, StartError>> + Unpin + Send + 'a
//     where
//         Self: 'a;

//     fn to_restart(&mut self) -> Self::ToRestartFut<'_>;
// }

// // enum ChildSpecV3<O>
// // where
// //     O: SpecifiesChildV3,
// // {
// //     /// When ready, the ChildSpec is ready to be started.
// //     /// Starting it will return `O::StartFut`
// //     Ready {
// //         link: Link,
// //         object: O,
// //     },
// //     /// The ChildSpec is currently starting.
// //     /// Awaiting the future returns `(Result<Child<E, P>, StartError>, O)`
// //     Starting {
// //         start_fut: O::StartFut,
// //     },
// //     Started,
// //     Temp,
// // }

// //------------------------------------------------------------------------------------------------
// //  V2
// //------------------------------------------------------------------------------------------------

// pub trait StartableV2 {
//     type Exit: Send + 'static;
//     type Protocol: Protocol;

//     type StartFut: Future<Output = (Result<Child<Self::Exit, Self::Protocol>, StartError>, Self)>
//         + Unpin
//         + Send;

//     type ToRestartFut: Future<Output = (Result<bool, StartError>, Self)> + Unpin + Send;

//     fn to_restart(self, exit: Result<Self::Exit, ExitError>) -> Self::ToRestartFut;

//     fn start(self, link: Link) -> Self::StartFut;
// }

// enum ChildSpecV2<O>
// where
//     O: StartableV2,
// {
//     /// When ready, the ChildSpec is ready to be started.
//     /// Starting it will return `O::StartFut`
//     Ready {
//         link: Link,
//         object: O,
//     },
//     /// The ChildSpec is currently starting.
//     /// Awaiting the future returns `(Result<Child<E, P>, StartError>, O)`
//     Starting {
//         start_fut: O::StartFut,
//     },
//     Started,
//     Temp,
// }

// impl<O: StartableV2> Unpin for ChildSpecV2<O> {}

// impl<O: StartableV2> Future for ChildSpecV2<O> {
//     type Output = Result<SupervisedChildV2<O>, StartError>;

//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         let mut child_spec = ChildSpecV2::Temp;
//         mem::swap(&mut *self, &mut child_spec);

//         let output = loop {
//             match child_spec {
//                 ChildSpecV2::Ready { link, object } => {
//                     let start_fut = object.start(link);
//                     child_spec = Self::Starting { start_fut };
//                 }
//                 ChildSpecV2::Starting { mut start_fut } => {
//                     let output = start_fut.poll_unpin(cx).map(|(res, object)| match res {
//                         Ok(child) => {
//                             mem::swap(&mut *self, &mut Self::Started);
//                             Ok(SupervisedChildV2::Alive { child, object })
//                         }
//                         Err(e) => Err(e),
//                     });

//                     child_spec = Self::Started;
//                     break output;
//                 }
//                 ChildSpecV2::Started => panic!("Can't start twice!"),
//                 ChildSpecV2::Temp => unreachable!(),
//             };
//         };

//         mem::swap(&mut *self, &mut child_spec);
//         output
//     }
// }

// enum SupervisedChildV2<O: StartableV2> {
//     /// The process has been started, and can now be supervised.
//     /// Awaiting the child returns a `Result<E, ExitError>`
//     Alive {
//         child: Child<O::Exit, O::Protocol>,
//         object: O,
//     },
//     /// The process has exited, and should now be asked whether it wants to restart.
//     /// Calling to_restart will return a `O::RestartFut`.
//     Exited {
//         child: Child<O::Exit, O::Protocol>,
//         exit: Result<O::Exit, ExitError>,
//         object: O,
//     },
//     /// The child is currently being polled whether it wants to restart.
//     /// This will return a `(Result<bool, StartError>, O)`
//     ToRestartFut {
//         child: Child<O::Exit, O::Protocol>,
//         object: O::ToRestartFut,
//     },
//     /// The child has decided whether it wants to restart.
//     /// If it does want to restart, it will return a
//     ToRestart {
//         child: Child<O::Exit, O::Protocol>,
//         to_restart: bool,
//         object: O,
//     },
//     /// The ChildSpec is currently starting.
//     /// Awaiting the future returns `(Result<Child<E, P>, StartError>, O)`
//     Restarting { object: O::StartFut },
// }

// impl<O: StartableV2> Unpin for SupervisedChildV2<O> {}
// impl<O: StartableV2> Stream for SupervisedChildV2<O> {
//     type Item = SupervisionItem;

//     fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//         todo!()
//         // loop {
//         //     match &mut *self {
//         //         SupervisedChildV2::Alive { child, object } => match child.poll_unpin(cx) {
//         //             Poll::Ready(exit) => mem::swap(
//         //                 &mut *self,
//         //                 &mut Self::Exited {
//         //                     child,
//         //                     exit,
//         //                     object,
//         //                 },
//         //             ),
//         //             Poll::Pending => break Poll::Pending,
//         //         },
//         //         SupervisedChildV2::Exited {
//         //             child,
//         //             exit,
//         //             object,
//         //         } => todo!(),
//         //         SupervisedChildV2::ToRestartFut { child, object } => todo!(),
//         //         SupervisedChildV2::ToRestart {
//         //             child,
//         //             to_restart,
//         //             object,
//         //         } => todo!(),
//         //         SupervisedChildV2::Restarting { object } => todo!(),
//         //     }
//         // }
//     }
// }

// enum SupervisionItem {
//     Finished,
//     Restart,
//     Failure(StartError),
// }

// struct BasicStartable<O, E, P, SFut, TRFut>
// where
//     E: Send + 'static,
//     P: Protocol,
//     SFut: Future<Output = Result<Child<E, P>, StartError>> + Unpin + Send,
//     TRFut: Future<Output = Result<bool, StartError>> + Unpin + Send,
// {
//     start: fn(&mut O, link: Link) -> SFut,
//     to_restart: fn(&mut O, exit: Result<E, ExitError>) -> TRFut,
//     object: O,
//     phantom: PhantomData<P>,
// }

// // impl<E, SFut, TRFut> StartableV2 for BasicStartable<E, SFut, TRFut> {
// //     type Exit = E;
// //     type Protocol;

// //     type StartFut<'a>
// //     where
// //         Self: 'a;

// //     type ToRestartFut<'a>
// //     where
// //         Self: 'a;

// //     fn to_restart(&mut self, exit: Result<Self::Exit, ExitError>) -> Self::ToRestartFut<'_> {
// //         todo!()
// //     }

// //     fn start(&mut self, link: Link) -> Self::StartFut<'_> {
// //         todo!()
// //     }
// // }

//------------------------------------------------------------------------------------------------
//  StartError
//------------------------------------------------------------------------------------------------

/// An error returned when starting of a process has failed.
///
/// This is a simple wrapper around a `Box<dyn Error + Send>`, and can therefore be used for any
/// type that implements `Error + Send + 'static`.
pub struct StartError(pub Box<dyn Error + Send>);

impl StartError {
    pub fn new<E: Error + Send + 'static>(error: E) -> Self {
        Self(Box::new(error))
        // Stream
    }
}

impl<E: Error + Send + 'static> From<E> for StartError {
    fn from(value: E) -> Self {
        Self::new(value)
    }
}
