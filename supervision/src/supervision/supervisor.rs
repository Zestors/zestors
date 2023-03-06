use crate as zestors;
use crate::all::*;
use futures::{Future, FutureExt, StreamExt, future::BoxFuture};
use pin_project::pin_project;
use std::{
    mem::swap,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::time::{sleep, Sleep};



//------------------------------------------------------------------------------------------------
//  SupervisorFut
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct SupervisorFut<S: Specification> {
    to_shutdown: bool,
    #[pin]
    state: SupervisorFutState<S>,
}

#[pin_project]
enum SupervisorFutState<S: Specification> {
    NotStarted(S),
    Starting(BoxFuture<'static, StartResult<S>>),
    Supervising(Pin<Box<S::Supervisee>>, Option<(Pin<Box<Sleep>>, bool)>),
    Exited,
}

impl<S: Specification> SupervisorFut<S> {
    pub fn new(spec: S) -> Self {
        Self {
            to_shutdown: false,
            state: SupervisorFutState::NotStarted(spec),
        }
    }

    pub fn shutdown(&mut self) {
        self.to_shutdown = true;
    }

    pub fn to_shutdown(&self) -> bool {
        self.to_shutdown
    }

    pub fn spawn_supervisor(self) -> (Child<SupervisionResult<S>>, SupervisorRef)
    where
        S: Sized + Send + 'static,
        S::Supervisee: Send,
    {
        SupervisorProcess::spawn_supervisor(self)
    }
}

impl<S: Specification> Future for SupervisorFut<S> {
    type Output = SupervisionResult<S>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        loop {
            match &mut this.state {
                SupervisorFutState::NotStarted(_spec) => {
                    let spec = {
                        let mut state = SupervisorFutState::Exited;
                        swap(&mut this.state, &mut state);
                        let SupervisorFutState::NotStarted (spec) = state else {
                            panic!()
                        };
                        spec
                    };

                    this.state = SupervisorFutState::Starting(Box::pin(spec.start_supervised()))
                }
                SupervisorFutState::Starting(fut) => {
                    if let Poll::Ready(res) = fut.as_mut().poll(cx) {
                        match res {
                            Err(StartError::Completed) => {
                                this.state = SupervisorFutState::Exited;
                                break Poll::Ready(Ok(None));
                            }
                            Ok((supervisee, _reference)) => {
                                this.state =
                                    SupervisorFutState::Supervising(Box::pin(supervisee), None);
                            }
                            Err(StartError::StartFailed(child_spec)) => {
                                this.state = SupervisorFutState::Exited;
                                break Poll::Ready(Ok(Some(child_spec)));
                            }
                            Err(StartError::Fatal(e)) => {
                                this.state = SupervisorFutState::Exited;
                                break Poll::Ready(Err(e));
                            }
                        };
                    };
                }

                SupervisorFutState::Supervising(supervisee, aborting) => match aborting {
                    Some((sleep, aborted)) => {
                        if let Poll::Ready(exit) = supervisee.as_mut().poll_supervise(cx) {
                            return Poll::Ready(exit);
                        }
                        if let Poll::Ready(()) = sleep.poll_unpin(cx) {
                            supervisee.as_mut().abort();
                            *aborted = true;
                        }
                        return Poll::Pending;
                    }
                    None => {
                        if this.to_shutdown {
                            supervisee.as_mut().halt();
                            *aborting = Some((
                                Box::pin(sleep(supervisee.as_ref().shutdown_time())),
                                false,
                            ));
                        } else {
                            return supervisee.as_mut().poll_supervise(cx);
                        }
                    }
                },

                SupervisorFutState::Exited => panic!("Already exited."),
            }
        }
    }
}

//------------------------------------------------------------------------------------------------
//  SupervisorActor
//------------------------------------------------------------------------------------------------

#[pin_project]
pub(super) struct SupervisorProcess<S: Specification> {
    #[pin]
    inbox: Inbox<SupervisorProtocol>,
    #[pin]
    supervision_fut: SupervisorFut<S>,
}

pub struct SupervisorRef {
    address: Address<Inbox<SupervisorProtocol>>,
}

#[protocol]
enum SupervisorProtocol {}

impl<S> SupervisorProcess<S>
where
    S: Specification + Send + 'static,
    S::Supervisee: Send,
{
    pub fn spawn_supervisor(
        supervision_fut: SupervisorFut<S>,
    ) -> (Child<SupervisionResult<S>>, SupervisorRef) {
        let (child, address) = spawn(|inbox: Inbox<SupervisorProtocol>| SupervisorProcess {
            inbox,
            supervision_fut,
        });
        (child.into_dyn(), SupervisorRef { address })
    }
}

impl<S: Specification> Future for SupervisorProcess<S> {
    type Output = SupervisionResult<S>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut proj = self.as_mut().project();

        if !proj.supervision_fut.to_shutdown() {
            if proj.inbox.halted() {
                proj.supervision_fut.shutdown();
            } else {
                if let Poll::Ready(res) = proj.inbox.next().poll_unpin(cx) {
                    match res {
                        Some(Ok(_msg)) => {
                            unreachable!("No messages handled yet");
                        }
                        Some(Err(Halted)) => proj.supervision_fut.shutdown(),
                        None => {
                            println!("WARN: Inbox of supervisor has been closed!");
                            proj.supervision_fut.shutdown();
                        }
                    }
                };
            }
        }

        proj.supervision_fut.poll(cx)
    }
}
