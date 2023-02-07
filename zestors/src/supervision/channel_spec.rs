use super::*;
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
pub struct RefSenderSpec<Sp: Spec> {
    #[pin]
    spec: Sp,
    sender: Option<mpsc::UnboundedSender<Sp::Ref>>,
}

impl<Sp: Spec> RefSenderSpec<Sp> {
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

impl<Sp: Spec> Spec for RefSenderSpec<Sp> {
    type Ref = ();
    type Supervisee = RefSenderSupervisee<Sp::Supervisee>;

    fn poll_start(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeStart<(Self::Supervisee, Self::Ref), Self>> {
        let proj = self.project();
        proj.spec.poll_start(cx).map(|start| {
            start
                .map_failure(|spec| RefSenderSpec {
                    spec,
                    sender: Some(proj.sender.take().unwrap()),
                })
                .map_success(|(supervisee, reference)| {
                    let sender = proj.sender.take().unwrap();
                    let _ = sender.send(reference);
                    (
                        RefSenderSupervisee {
                            supervisee,
                            sender: Some(sender),
                        },
                        (),
                    )
                })
        })
    }

    fn start_timeout(self: Pin<&Self>) -> Duration {
        self.project_ref().spec.start_timeout()
    }
}

//------------------------------------------------------------------------------------------------
//  Supervisee
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct RefSenderSupervisee<S: Supervisee> {
    #[pin]
    supervisee: S,
    sender: Option<mpsc::UnboundedSender<<S::Spec as Spec>::Ref>>,
}

impl<S: Supervisee> Supervisee for RefSenderSupervisee<S> {
    type Spec = RefSenderSpec<S::Spec>;

    fn poll_supervise(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeExit<Self::Spec>> {
        let proj = self.project();
        proj.supervisee.poll_supervise(cx).map(|res| {
            res.map_exit(|spec| RefSenderSpec {
                spec,
                sender: proj.sender.take(),
            })
        })
    }

    fn abort_timeout(self: Pin<&Self>) -> Duration {
        self.project_ref().supervisee.abort_timeout()
    }

    fn halt(self: Pin<&mut Self>) {
        self.project().supervisee.halt()
    }

    fn abort(self: Pin<&mut Self>) {
        self.project().supervisee.abort()
    }
}
