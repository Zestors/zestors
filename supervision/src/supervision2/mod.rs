#[allow(unused)]
use crate::all::*;
use async_trait::async_trait;
use futures::{future::BoxFuture, Future, FutureExt};
use pin_project::pin_project;
use std::{
    mem::swap,
    pin::Pin,
    task::{Context, Poll},
    time::Duration, sync::Mutex,
};

struct MyProcessId;


trait ActorIdentifier {
    fn to_id(&self) -> u64;
}

struct BoxSpec;

struct OneForOneSpec {
    specs: Mutex<Vec<BoxSpec>>
}

#[async_trait]
pub trait Specification: Clone + Sized {
    type Supervisee;
    type Ref;
    type Error;

    async fn start_supervised(&self) -> Result<(Self::Supervisee, Self::Ref), Self::Error>;
    fn start_duration(&self) -> Duration {
        Duration::from_secs(1)
    }
}

pub trait Supervisee: Send + Sized {
    type Supervisee: Specification<Supervisee = Self>;
    type Exit;

    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context) -> Poll<(Self::Exit, bool)>;

    fn shutdown_time(self: Pin<&Self>) -> Duration;

    fn halt(self: Pin<&mut Self>);

    fn abort(self: Pin<&mut Self>);
}

//------------------------------------------------------------------------------------------------
//  OLD STUFF
//------------------------------------------------------------------------------------------------

// mod box_supervisee;

// #[allow(unused)]
// use crate::all::*;
// use async_trait::async_trait;
// use futures::{future::BoxFuture, Future, FutureExt};
// use pin_project::pin_project;
// use std::{
//     mem::swap,
//     pin::Pin,
//     task::{Context, Poll},
//     time::Duration,
// };

// use self::box_supervisee::BoxSupervisee;

// pub trait Supervisee: Send + Sized + 'static {
//     type Ref: Send + 'static;

//     type Running: RunningSupervisee<Supervisee = Self>;

//     type Future: Future<Output = Result<(Self::Running, Self::Ref), Self>> + Send + 'static;

//     fn start_supervised(self) -> Self::Future;
// }

// pub trait RunningSupervisee: Send + Sized {
//     type Supervisee: Supervisee<Running = Self>;

//     fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Supervisee>>;

//     fn shutdown_time(self: Pin<&Self>) -> Duration;

//     fn halt(self: Pin<&mut Self>);

//     fn abort(self: Pin<&mut Self>);
// }

// #[pin_project]
// pub enum SuperviseeState<S: Supervisee = BoxSupervisee> {
//     Startable(S),
//     Starting(#[pin] BoxFuture<'static, Result<(S::Running, S::Ref), S>>),
//     Running(#[pin] S::Running, bool),
//     Complete,
// }

// pub struct OneForOneSupervisee {
//     supervisees: Vec<SuperviseeState>,
// }

// impl Supervisee for OneForOneSupervisee {
//     type Ref = ();
//     type Running = RunningOneForOneSupervisee;
//     type Future = OneForOneSuperviseeFuture;

//     fn start_supervised(self) -> Self::Future {
//         OneForOneSuperviseeFuture {
//             inner: Some(self),
//             canceling: false,
//         }
//     }
// }

// pub struct OneForOneSuperviseeFuture {
//     inner: Option<OneForOneSupervisee>,
//     canceling: bool,
// }

// impl Future for OneForOneSuperviseeFuture {
//     type Output = Result<(RunningOneForOneSupervisee, ()), OneForOneSupervisee>;

//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         let this = &mut *self;
//         for supervisee in &mut this.inner.as_mut().unwrap().supervisees {
//             'this_supervisee: loop {
//                 match supervisee {
//                     SuperviseeState::Startable(_) => {
//                         if this.canceling {
//                             break 'this_supervisee;
//                         }
//                         let SuperviseeState::Startable(s) = std::mem::replace(supervisee, SuperviseeState::Complete) else {
//                             unreachable!()
//                         };
//                         *supervisee = SuperviseeState::Starting(s.start_supervised());
//                     }
//                     SuperviseeState::Starting(s) => {
//                         if let Poll::Ready(start_res) = s.poll_unpin(cx) {
//                             match start_res {
//                                 Ok((running, ())) => {
//                                     *supervisee = SuperviseeState::Running(running, false);
//                                     break 'this_supervisee;
//                                 }
//                                 Err(s) => {
//                                     *supervisee = SuperviseeState::Startable(s);
//                                     this.canceling = true;
//                                     break 'this_supervisee;
//                                 }
//                             }
//                         }
//                     }
//                     SuperviseeState::Running(s, halted) => {
//                         if !this.canceling {
//                             break;
//                         }
//                     }
//                     SuperviseeState::Complete => break,
//                 }
//             }
//         }

//         todo!()
//     }
// }

// pub struct RunningOneForOneSupervisee {
//     inner: Option<OneForOneSupervisee>,
// }

// impl RunningSupervisee for RunningOneForOneSupervisee {
//     type Supervisee = OneForOneSupervisee;

//     fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Supervisee>> {
//         todo!()
//     }

//     fn shutdown_time(self: Pin<&Self>) -> Duration {
//         todo!()
//     }

//     fn halt(self: Pin<&mut Self>) {
//         todo!()
//     }

//     fn abort(self: Pin<&mut Self>) {
//         todo!()
//     }
// }
