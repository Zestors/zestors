use super::*;
use futures::Future;
use pin_project::pin_project;
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::mpsc;

//------------------------------------------------------------------------------------------------
//  Spec
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct RefSenderSpec<S: Specification> {
    #[pin]
    spec: S,
    sender: Option<mpsc::UnboundedSender<S::Ref>>,
}

#[pin_project]
pub struct RefSenderSpecFut<S: Specification> {
    #[pin]
    fut: S::StartFut,
    sender: Option<mpsc::UnboundedSender<S::Ref>>,
}

#[pin_project]
pub struct RefSenderSupervisee<S: Specification> {
    #[pin]
    supervisee: S::Supervisee,
    sender: Option<mpsc::UnboundedSender<S::Ref>>,
}

impl<Sp: Specification> RefSenderSpec<Sp> {
    pub fn new(spec: Sp) -> (Self, mpsc::UnboundedReceiver<Sp::Ref>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self::new_with_channel(spec, sender), receiver)
    }

    pub fn new_with_channel(spec: Sp, sender: mpsc::UnboundedSender<Sp::Ref>) -> Self {
        Self {
            spec,
            sender: Some(sender),
        }
    }
}

impl<Sp: Specification> Specification for RefSenderSpec<Sp> {
    type Ref = ();
    type Supervisee = RefSenderSupervisee<Sp>;

    fn start_time(&self) -> Duration {
        self.spec.start_time()
    }

    fn start(self) -> Self::StartFut {
        RefSenderSpecFut {
            fut: self.spec.start(),
            sender: self.sender,
        }
    }

    type StartFut = RefSenderSpecFut<Sp>;
}

impl<Sp: Specification> Future for RefSenderSpecFut<Sp> {
    type Output = StartResult<RefSenderSpec<Sp>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let proj = self.project();
        proj.fut.poll(cx).map(|start| match start {
            Ok((supervisee, reference)) => {
                let sender = proj.sender.take().unwrap();
                let _ = sender.send(reference);
                Ok((
                    RefSenderSupervisee {
                        supervisee,
                        sender: Some(sender),
                    },
                    (),
                ))
            }
            Err(StartError::Failed(spec)) => Err(StartError::Failed(RefSenderSpec {
                spec,
                sender: Some(proj.sender.take().unwrap()),
            })),
            Err(StartError::Completed) => Err(StartError::Completed),
            Err(StartError::Irrecoverable(e)) => Err(StartError::Irrecoverable(e)),
        })
    }
}

//------------------------------------------------------------------------------------------------
//  Supervisee
//------------------------------------------------------------------------------------------------

impl<S: Specification> Supervisee for RefSenderSupervisee<S> {
    type Spec = RefSenderSpec<S>;

    fn shutdown_time(self: Pin<&Self>) -> Duration {
        self.project_ref().supervisee.shutdown_time()
    }

    fn halt(self: Pin<&mut Self>) {
        self.project().supervisee.halt()
    }

    fn abort(self: Pin<&mut Self>) {
        self.project().supervisee.abort()
    }
}

impl<S: Specification> Future for RefSenderSupervisee<S> {
    type Output = ExitResult<RefSenderSpec<S>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let proj = self.project();
        proj.supervisee.poll(cx).map(|res| {
            res.map(|spec| {
                spec.map(|spec| RefSenderSpec {
                    spec,
                    sender: proj.sender.take(),
                })
            })
        })
    }
}
