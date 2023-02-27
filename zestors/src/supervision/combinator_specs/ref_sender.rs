use super::*;
use futures::Future;
use pin_project::pin_project;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;

//------------------------------------------------------------------------------------------------
//  Spec
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct RefSenderSpec<S: Specifies> {
    #[pin]
    spec: S,
    sender: Option<mpsc::UnboundedSender<S::Ref>>,
}

#[pin_project]
pub struct RefSenderSpecFut<S: Specifies> {
    #[pin]
    fut: S::Fut,
    sender: Option<mpsc::UnboundedSender<S::Ref>>,
}

#[pin_project]
pub struct RefSenderSupervisee<S: Specifies> {
    #[pin]
    supervisee: S::Supervisee,
    sender: Option<mpsc::UnboundedSender<S::Ref>>,
}

impl<Sp: Specifies> RefSenderSpec<Sp> {
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

impl<Sp: Specifies> Specifies for RefSenderSpec<Sp> {
    type Ref = ();
    type Supervisee = RefSenderSupervisee<Sp>;

    fn start(self) -> Self::Fut {
        RefSenderSpecFut {
            fut: self.spec.start(),
            sender: self.sender,
        }
    }

    type Fut = RefSenderSpecFut<Sp>;
}

impl<Sp: Specifies> Future for RefSenderSpecFut<Sp> {
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

impl<S: Specifies> Supervisable for RefSenderSupervisee<S> {
    type Spec = RefSenderSpec<S>;

    fn shutdown_time(self: Pin<&Self>) -> ShutdownDuration {
        self.project_ref().supervisee.shutdown_time()
    }

    fn halt(self: Pin<&mut Self>) {
        self.project().supervisee.halt()
    }

    fn abort(self: Pin<&mut Self>) {
        self.project().supervisee.abort()
    }

    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context) -> Poll<SuperviseResult<Self::Spec>> {
        let proj = self.project();
        proj.supervisee.poll_supervise(cx).map(|res| {
            res.map(|spec| {
                spec.map(|spec| RefSenderSpec {
                    spec,
                    sender: proj.sender.take(),
                })
            })
        })
    }
}
