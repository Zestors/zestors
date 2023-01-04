use crate::*;
use futures::{future::BoxFuture, pin_mut, Future, FutureExt, Stream};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

//------------------------------------------------------------------------------------------------
//  DynamicSupervisee
//------------------------------------------------------------------------------------------------

pub struct DynamicSupervisee(
    Box<dyn for<'a> PrivSupervises<'a, SuperviseFut = BoxFuture<'a, Result<bool, StartError>>>>,
);

impl DynamicSupervisee {
    pub fn new<S: Restartable>(inner: Supervisee<S>) -> Self {
        Self(Box::new(inner))
    }

    pub async fn supervise(&mut self) -> Result<bool, StartError> {
        self.0.supervise().await
    }
}

impl Unpin for DynamicSupervisee {}

impl Stream for DynamicSupervisee {
    type Item = Result<(), StartError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.supervise().poll_unpin(cx).map(|res| match res {
            Ok(a) => match a {
                true => Some(Ok(())),
                false => None,
            },
            Err(e) => Some(Err(e)),
        })
    }
}

trait PrivSupervises<'a>: Send + 'static {
    type SuperviseFut: Future<Output = Result<bool, StartError>> + Send + 'a;
    fn supervise(&'a mut self) -> Self::SuperviseFut;
}

impl<R: Restartable> From<Supervisee<R>> for DynamicSupervisee {
    fn from(value: Supervisee<R>) -> Self {
        value.into_dyn()
    }
}

//------------------------------------------------------------------------------------------------
//  Supervisee
//------------------------------------------------------------------------------------------------

/// A child that can be restarted automatically. See [Self::supervise] for more details.
pub struct Supervisee<R: Restartable> {
    child: Child<R::Exit, R::ActorType>,
    state: Option<RestartableChildState<R>>,
}

enum RestartableChildState<R: Restartable> {
    /// The process is alive.
    Alive(R),
    /// The process has exited, but does not know if it wants to be restarted or not.
    Exited(R::Fut),
    /// The cprocess wants to be restarted.
    WantsRestart(R::Starter),
    /// The ChildSpec is currently starting.
    /// Awaiting the future returns `(Result<Child<E, P>, StartError>, O)`
    Restarting(<R::Starter as StartableWith<R::Starter>>::Fut),
    /// The child has exited and does not want to be restarted.
    Finished,
    /// Starting has failed, it is impossible to start this again.
    StartFailed,
}

impl<R: Restartable> Supervisee<R> {
    pub fn new(child: Child<R::Exit, R::ActorType>, restartable: R) -> Self {
        assert!(!child.is_finished());
        Self {
            child,
            state: Some(RestartableChildState::Alive(restartable)),
        }
    }

    pub fn into_dyn(self) -> DynamicSupervisee {
        DynamicSupervisee::new(self)
    }
    /// Supervise the process and automatically restart it by calling this again.
    ///
    /// Calling `supervise()` may return the following results:
    /// - `Ok(true)` -> Process has exited and wants to be restarted. Restarting can
    /// be done by calling `supervise()` again.
    /// - `Ok(false)` -> Process has exited and does not want to be restarted. Calling
    /// `supervise()` again will cause a `panic`.
    /// - `Err(StartError)` -> Attempt to (re)start the process has failed, and it can never
    /// be restarted. Calling `supervise()` again will cause a `panic`.
    pub async fn supervise(&mut self) -> Result<bool, StartError> {
        loop {
            match self.state.take().unwrap() {
                RestartableChildState::Alive(supervisable) => {
                    let exit = (&mut self.child).await;
                    let should_restart_fut = supervisable.should_restart(exit);
                    self.state = Some(RestartableChildState::Exited(should_restart_fut));
                }
                RestartableChildState::Exited(should_restart_fut) => {
                    pin_mut!(should_restart_fut);
                    match should_restart_fut.await {
                        Ok(Some(startable)) => {
                            self.state = Some(RestartableChildState::WantsRestart(startable));
                            break Ok(true);
                        }
                        Ok(None) => {
                            self.state = Some(RestartableChildState::Finished);
                            break Ok(false);
                        }
                        Err(e) => {
                            self.state = Some(RestartableChildState::StartFailed);
                            break Err(e);
                        }
                    }
                }
                RestartableChildState::WantsRestart(startable) => {
                    let fut = startable.start();
                    self.state = Some(RestartableChildState::Restarting(fut))
                }
                RestartableChildState::Restarting(start_fut) => {
                    pin_mut!(start_fut);
                    match start_fut.await {
                        Ok((child, supervisable)) => {
                            self.child = child;
                            self.state = Some(RestartableChildState::Alive(supervisable));
                        }
                        Err(e) => {
                            self.state = Some(RestartableChildState::StartFailed);
                            break Err(e);
                        }
                    }
                }
                RestartableChildState::Finished => panic!("Process was already finished!"),
                RestartableChildState::StartFailed => panic!("Starting has failed before!"),
            }
        }
    }
}

impl<'a, S: Restartable> PrivSupervises<'a> for Supervisee<S> {
    type SuperviseFut = BoxFuture<'a, Result<bool, StartError>>;

    fn supervise(&'a mut self) -> Self::SuperviseFut {
        Box::pin(async move { self.supervise().await })
    }
}
