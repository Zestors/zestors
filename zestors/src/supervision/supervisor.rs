use super::*;
use futures::{ready, Future, FutureExt, StreamExt};
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
    Starting(Pin<Box<S::Fut>>, Pin<Box<Sleep>>),
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

    pub fn spawn_supervisor(self) -> (Child<ExitResult<S>>, SupervisorRef)
    where
        S: Sized + Send + 'static,
        S::Supervisee: Send,
        S::Fut: Send,
    {
        SupervisorProcess::spawn_supervisor(self)
    }
}

impl<S: Specification> Future for SupervisorFut<S> {
    type Output = ExitResult<S>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        loop {
            match &mut this.state {
                SupervisorFutState::NotStarted(spec) => {
                    let timeout = Box::pin(sleep(spec.start_timeout()));

                    let spec = {
                        let mut state = SupervisorFutState::Exited;
                        swap(&mut this.state, &mut state);
                        let SupervisorFutState::NotStarted (spec) = state else {
                            panic!()
                        };
                        spec
                    };

                    this.state = SupervisorFutState::Starting(Box::pin(spec.start()), timeout)
                }
                SupervisorFutState::Starting(fut, start_timer) => {
                    if let Poll::Ready(res) = fut.as_mut().poll(cx) {
                        match res {
                            Err(StartError::Finished) => {
                                this.state = SupervisorFutState::Exited;
                                break Poll::Ready(Ok(None));
                            }
                            Ok((supervisee, _reference)) => {
                                this.state =
                                    SupervisorFutState::Supervising(Box::pin(supervisee), None);
                            }
                            Err(StartError::Failure(child_spec)) => {
                                this.state = SupervisorFutState::Exited;
                                break Poll::Ready(Ok(Some(child_spec)));
                            }
                            Err(StartError::Unhandled(e)) => {
                                this.state = SupervisorFutState::Exited;
                                break Poll::Ready(Err(e));
                            }
                        };
                    } else {
                        break match start_timer.as_mut().poll_unpin(cx) {
                            Poll::Ready(()) => {
                                this.state = SupervisorFutState::Exited;
                                Poll::Ready(todo!())
                            }
                            Poll::Pending => Poll::Pending,
                        };
                    }
                }

                SupervisorFutState::Supervising(supervisee, aborting) => match aborting {
                    Some((sleep, aborted)) => {
                        if let Poll::Ready(exit) = supervisee.as_mut().poll(cx) {
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
                            *aborting =
                                Some((Box::pin(sleep(supervisee.as_ref().abort_timeout())), false));
                        } else {
                            return supervisee.as_mut().poll(cx);
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
pub(super) struct SupervisorProcess<Sp: Specification> {
    #[pin]
    inbox: Inbox<SupervisorProtocol>,
    #[pin]
    supervision_fut: SupervisorFut<Sp>,
}

pub struct SupervisorRef {
    address: Address<Inbox<SupervisorProtocol>>,
}

#[protocol]
enum SupervisorProtocol {}

impl<Sp: Specification> SupervisorProcess<Sp> {
    pub fn spawn_supervisor(
        supervision_fut: SupervisorFut<Sp>,
    ) -> (Child<ExitResult<Sp>>, SupervisorRef)
    where
        Sp: Send + 'static,
        Sp::Supervisee: Send,
        Sp::Fut: Send,
    {
        let (child, address) = spawn(|inbox: Inbox<SupervisorProtocol>| SupervisorProcess {
            inbox,
            supervision_fut,
        });
        (child.into_dyn(), SupervisorRef { address })
    }
}

impl<S: Specification> Future for SupervisorProcess<S> {
    type Output = ExitResult<S>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut proj = self.as_mut().project();

        if !proj.supervision_fut.to_shutdown() {
            if proj.inbox.is_halted() {
                proj.supervision_fut.shutdown();
            } else {
                if let Poll::Ready(res) = proj.inbox.next().poll_unpin(cx) {
                    match res {
                        Some(Ok(_msg)) => {
                            unreachable!("No messages handled yet");
                        }
                        Some(Err(HaltedError)) => proj.supervision_fut.shutdown(),
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
