//! # todo
//!
//! | __<--__ [`inboxes`] | [`distribution`] __-->__ |
//! |---|---|
mod channel_spec;
mod restart_limiter;
mod supervisor;
mod traits;
mod fn_spec;
pub use channel_spec::*;
pub use restart_limiter::*;
pub use supervisor::*;
pub use traits::*;
pub use fn_spec::*;

use crate as zestors;
#[allow(unused)]
use crate::*;
use pin_project::pin_project;
use std::{
    error::Error,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

type BoxError = Box<dyn Error + Send>;

//------------------------------------------------------------------------------------------------
//  Group
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct OneForOnceSpec<T: Spec = DynSpec> {
    starting: Vec<T>,
    start_failed: Vec<T>,
    started: Vec<T::Supervisee>,
    errors: Vec<BoxError>,
    limiter: RestartLimiter,
    abort_deadline: Option<Instant>,
}

#[pin_project]
pub struct OneForOneSupervisee<T: Supervisee = DynSupervisee> {
    supervising: Vec<T>,
    exited: Vec<T::Spec>,
    errors: Vec<BoxError>,
}

impl<T: Spec> Spec for OneForOnceSpec<T> {
    type Ref = Vec<T::Ref>;
    type Supervisee = OneForOneSupervisee<T::Supervisee>;

    fn poll_start(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeStart<(Self::Supervisee, Self::Ref), Self>> {
        let proj = self.project();

        if !proj.starting.is_empty() {
            for child_spec in proj.starting {
                if let Poll::Ready(res) = Pin::new(child_spec).poll_start(cx) {
                    match res {
                        SuperviseeStart::Finished => (),
                        SuperviseeStart::Failure(child_spec) => {
                            if !proj.limiter.restart_within_limit() {
                                proj.start_failed.push(child_spec)
                            }
                        }
                        SuperviseeStart::Succesful(supervisee) => proj.started.push(supervisee.0),
                        SuperviseeStart::UnrecoverableFailure(e) => proj.errors.push(e),
                    }
                }
            }
        } else {
            if proj.limiter.triggered() {
            } else {
                todo!()
            }
        }

        todo!()
    }

    fn start_timeout(self: Pin<&Self>) -> Duration {
        todo!()
    }
}

impl<T: Supervisee> Supervisee for OneForOneSupervisee<T> {
    type Spec = OneForOnceSpec<T::Spec>;

    fn poll_supervise(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeExit<Self::Spec>> {
        todo!()
    }

    fn halt(self: Pin<&mut Self>) {
        todo!()
    }

    fn abort(self: Pin<&mut Self>) {
        todo!()
    }

    fn abort_timeout(self: Pin<&Self>) -> Duration {
        todo!()
    }
}
