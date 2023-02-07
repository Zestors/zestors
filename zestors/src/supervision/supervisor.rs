use super::*;
use futures::{ready, Future, FutureExt, StreamExt};
use pin_project::pin_project;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::time::{sleep, Sleep};

pub struct SupervisorSpec<SS: Spec> {
    supervision_spec: SS,
}

pub struct SupervisorSupervisee<SS: Spec + Send + 'static> {
    child: Child<SuperviseeExit<SS>>,
}

impl<SS: Spec> SupervisorSpec<SS> {
    pub fn new(spec: SS) -> Self {
        Self {
            supervision_spec: spec,
        }
    }
}

impl<SS: Spec + Send + 'static> Spec for SupervisorSpec<SS> {
    type Ref = ();
    type Supervisee = SupervisorSupervisee<SS>;

    fn poll_start(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeStart<(Self::Supervisee, Self::Ref), Self>> {
        todo!()
    }

    fn start_timeout(self: Pin<&Self>) -> Duration {
        todo!()
    }
}

impl<SS: Spec + Send + 'static> Supervisee for SupervisorSupervisee<SS> {
    type Spec = SupervisorSpec<SS>;

    fn poll_supervise(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeExit<Self::Spec>> {
        todo!()
    }

    fn abort_timeout(self: Pin<&Self>) -> Duration {
        todo!()
    }

    fn halt(self: Pin<&mut Self>) {
        todo!()
    }

    fn abort(self: Pin<&mut Self>) {
        todo!()
    }
}

//------------------------------------------------------------------------------------------------
//  SupervisorActor
//------------------------------------------------------------------------------------------------

#[pin_project]
pub(super) struct SupervisorProcess<SS: Spec> {
    #[pin]
    inbox: Inbox<SupervisorProtocol>,
    #[pin]
    supervision_fut: SupervisionFut<SS>,
}

pub struct SupervisorRef {
    address: Address<Inbox<SupervisorProtocol>>,
}

#[protocol]
enum SupervisorProtocol {}

impl<SS: Spec> SupervisorProcess<SS> {
    pub fn spawn_supervisor(
        supervision_fut: SupervisionFut<SS>,
    ) -> (Child<SuperviseeExit<SS>>, SupervisorRef)
    where
        SS: Send + 'static,
        SS::Supervisee: Send,
    {
        let (child, address) = spawn(|inbox: Inbox<SupervisorProtocol>| SupervisorProcess {
            inbox,
            supervision_fut,
        });
        (child.into_dyn(), SupervisorRef { address })
    }
}

impl<SS: Spec> Future for SupervisorProcess<SS> {
    type Output = SuperviseeExit<SS>;

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

//------------------------------------------------------------------------------------------------
//  Supervisor
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct SupervisionFut<SS: Spec> {
    to_shutdown: bool,
    #[pin]
    state: SupervisorState<SS>,
}

#[pin_project]
enum SupervisorState<SS: Spec> {
    Starting {
        supervisee_spec: Pin<Box<SS>>,
        canceling: Option<Pin<Box<Sleep>>>,
    },
    Supervising {
        supervisee: Pin<Box<SS::Supervisee>>,
        aborting: Option<(Pin<Box<Sleep>>, bool)>,
    },
    Exited,
}

impl<SS: Spec> SupervisionFut<SS> {
    pub fn new(supervision_spec: SS) -> Self {
        Self {
            to_shutdown: false,
            state: SupervisorState::Starting {
                supervisee_spec: Box::pin(supervision_spec),
                canceling: None,
            },
        }
    }

    pub fn shutdown(&mut self) {
        self.to_shutdown = true;
    }

    pub fn to_shutdown(&self) -> bool {
        self.to_shutdown
    }

    pub fn spawn_supervisor(self) -> (Child<SuperviseeExit<SS>>, SupervisorRef)
    where
        SS: Sized + Send + 'static,
        SS::Supervisee: Send,
    {
        SupervisorProcess::spawn_supervisor(self)
    }
}

impl<Sp: Spec> Future for SupervisionFut<Sp> {
    type Output = SuperviseeExit<Sp>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut proj = self.project();

        loop {
            match &mut *proj.state {
                SupervisorState::Starting {
                    supervisee_spec,
                    canceling,
                } => match canceling {
                    Some(start_timer) => {
                        if let Poll::Ready(start) = supervisee_spec.as_mut().poll_start(cx) {
                            match start {
                                SuperviseeStart::Finished => {
                                    *proj.state = SupervisorState::Exited;
                                    return Poll::Ready(SuperviseeExit::Finished);
                                }
                                SuperviseeStart::Succesful((supervisee, _reference)) => {
                                    *proj.state = SupervisorState::Supervising {
                                        supervisee: Box::pin(supervisee),
                                        aborting: None,
                                    };
                                }
                                SuperviseeStart::Failure(child_spec) => {
                                    *proj.state = SupervisorState::Exited;
                                    return Poll::Ready(SuperviseeExit::Exit(child_spec));
                                }
                                SuperviseeStart::UnrecoverableFailure(e) => {
                                    *proj.state = SupervisorState::Exited;
                                    return Poll::Ready(SuperviseeExit::UnrecoverableFailure(e));
                                }
                            };
                        } else {
                            return match start_timer.as_mut().poll_unpin(cx) {
                                Poll::Ready(()) => {
                                    *proj.state = SupervisorState::Exited;
                                    Poll::Ready(SuperviseeExit::UnrecoverableFailure(todo!()))
                                }
                                Poll::Pending => Poll::Pending,
                            };
                        }
                    }
                    None => {
                        *canceling = Some(Box::pin(sleep(supervisee_spec.as_ref().start_timeout())))
                    }
                },

                SupervisorState::Supervising {
                    supervisee,
                    aborting,
                } => match aborting {
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
                        if *proj.to_shutdown {
                            supervisee.as_mut().halt();
                            *aborting =
                                Some((Box::pin(sleep(supervisee.as_ref().abort_timeout())), false));
                        } else {
                            return supervisee.as_mut().poll_supervise(cx);
                        }
                    }
                },

                SupervisorState::Exited => panic!("Already exited."),
            }
        }
    }
}
