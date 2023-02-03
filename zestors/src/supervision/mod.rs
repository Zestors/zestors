//! # todo
//!
//! | __<--__ [`inboxes`] | [`distribution`] __-->__ |
//! |---|---|
use std::{
    convert::Infallible,
    error::Error,
    fmt::Debug,
    mem,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

#[allow(unused)]
use crate::*;
use anyhow::anyhow;
use futures::{
    future::{ready, BoxFuture, Ready},
    pending, pin_mut, ready, Future, FutureExt, Stream,
};
use pin_project::pin_project;
use tokio::time::Instant;

//------------------------------------------------------------------------------------------------
//  RestartCounter
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct RestartCounter {
    limit: usize,
    within: Duration,
    values: Vec<Instant>,
}

impl RestartCounter {
    /// Create a new restart_counter with a
    pub fn new(limit: usize, within: Duration) -> Self {
        Self {
            limit,
            within,
            values: Vec::new(),
        }
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }

    pub fn set_within(&mut self, within: Duration) {
        self.within = within;
    }

    pub fn is_within_limit(&mut self) -> bool {
        self.values
            .retain(|instant| instant.elapsed() < self.within);
        self.values.len() <= self.limit
    }

    pub fn add_is_within_limit(&mut self) -> bool {
        self.values.push(Instant::now());
        self.is_within_limit()
    }
}

//------------------------------------------------------------------------------------------------
//  Traits
//------------------------------------------------------------------------------------------------

mod seperate_traits {
    use super::BoxError;
    use crate::{self as zestors, ActorRefExt};
    use crate::{spawn, Address, Child};
    use async_trait::async_trait;
    use futures::{pin_mut, ready, Future, FutureExt, StreamExt};
    use pin_project::pin_project;
    use std::mem;
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };
    use zestors_codegen::protocol;
    use zestors_extra::inbox::{HaltedError, Inbox, RecvError};

    //------------------------------------------------------------------------------------------------
    //  Static
    //------------------------------------------------------------------------------------------------

    pub trait Supervisable {
        type ChildSpec: SpecifiesChild<Supervisee = Self>;

        /// Supervise the supervisee until they exit with:
        /// - `Some(Self::Starter)`: The actor would like to be restarted using
        /// the starter.
        /// - `None`: The actor is finished, and does not need to be restarted.
        ///
        /// After this method has been polled to completion, this may not be polled again.
        fn poll_supervise(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Option<Self::ChildSpec>>;

        /// Start shutting down the supervisee and it's actors. Calling `poll_supervise`
        /// afterwards will continue the shutdown until successful.
        ///
        /// If the shutdown is taking too long, `abort` may be called as well.
        fn poll_shutdown(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Option<Self::ChildSpec>>;

        /// Aborts all actors.
        fn abort(self: Pin<&mut Self>);
    }

    pub trait SpecifiesChild {
        type Supervisee: Supervisable<ChildSpec = Self>;

        /// Starts the actor, with the following possible outcomes.
        /// - `Ok(Some(Self::Supervisee))`: The supervisee was successfully started and can be
        /// supervised with the `Supervisee`.
        /// - `Ok(None)`: The supervisee is finished, and does not need to be started.
        /// - `Err(BoxError)`: The supervisee failed to start, but would like to be restarted under
        /// different external conditions.
        fn poll_start(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<Option<Self::Supervisee>, BoxError>>;
    }

    //------------------------------------------------------------------------------------------------
    //  Dynamic
    //------------------------------------------------------------------------------------------------

    pub trait DynSupervisable: Send + 'static {
        fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Restarter>>;
        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Restarter>>;
        fn abort(self: Pin<&mut Self>);
    }

    pub trait DynSpecifiesChild: Send + 'static {
        fn poll_start(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<Option<ChildSpec>, BoxError>>;
    }

    //------------------------------------------------------------------------------------------------
    //  Dynamic implementations
    //------------------------------------------------------------------------------------------------

    impl<T> DynSupervisable for T
    where
        T: Supervisable + Send + 'static,
        T::ChildSpec: Send + 'static,
    {
        fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Restarter>> {
            Supervisable::poll_supervise(self, cx).map(|restarter| {
                restarter.map(|restarter| {
                    let restarter: Restarter = Box::pin(restarter);
                    restarter
                })
            })
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Restarter>> {
            Supervisable::poll_shutdown(self, cx).map(|restarter| {
                restarter.map(|restarter| {
                    let restarter: Restarter = Box::pin(restarter);
                    restarter
                })
            })
        }

        fn abort(self: Pin<&mut Self>) {
            Supervisable::abort(self)
        }
    }

    impl<T> DynSpecifiesChild for T
    where
        T: SpecifiesChild + Send + 'static,
        T::Supervisee: Send + 'static,
    {
        fn poll_start(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<Option<ChildSpec>, BoxError>> {
            SpecifiesChild::poll_start(self, cx).map_ok(|supervisee| {
                supervisee.map(|supervisee| {
                    let supervisee: ChildSpec = Box::pin(supervisee);
                    supervisee
                })
            })
        }
    }

    type Restarter = Pin<Box<dyn DynSpecifiesChild>>;
    type ChildSpec = Pin<Box<dyn DynSupervisable>>;

    //------------------------------------------------------------------------------------------------
    //  Supervisor
    //------------------------------------------------------------------------------------------------

    pub struct Supervisor<T>(T);

    impl<T> Supervisor<T> {
        pub fn new(restartable: T) -> Self {
            Self(restartable)
        }

        pub fn start(self) -> (Child<Option<Pin<Box<T>>>>, SupervisorRef)
        where
            T: SpecifiesChild + Sized + Send + 'static,
            T::Supervisee: Send,
        {
            let (child, address) = spawn(|inbox: Inbox<SupervisorProtocol>| SupervisorFut {
                inbox,
                state: SupervisorState::Starting(Box::pin(self.0)),
            });
            (child.into_dyn(), SupervisorRef { address })
        }
    }

    pub struct SupervisorRef {
        address: Address<Inbox<SupervisorProtocol>>,
    }

    #[protocol]
    enum SupervisorProtocol {}

    //------------------------------------------------------------------------------------------------
    //  SupervisorFut
    //------------------------------------------------------------------------------------------------

    #[pin_project]
    struct SupervisorFut<T: SpecifiesChild> {
        #[pin]
        inbox: Inbox<SupervisorProtocol>,
        state: SupervisorState<T>,
    }

    #[pin_project]
    enum SupervisorState<T: SpecifiesChild> {
        Starting(Pin<Box<T>>),
        Supervising(Pin<Box<T::Supervisee>>),
        Exited,
    }

    impl<T: SpecifiesChild> Future for SupervisorFut<T> {
        type Output = Option<Pin<Box<T>>>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut proj = self.as_mut().project();

            loop {
                let to_shutdown = if !proj.inbox.is_halted() && !proj.inbox.is_closed() {
                    if let Poll::Ready(res) = proj.inbox.next().poll_unpin(cx) {
                        match res {
                            Some(Ok(_msg)) => {
                                unreachable!("No messages handled yet");
                            }
                            Some(Err(HaltedError)) => true,
                            None => {
                                println!("WARN: Inbox of supervisor has been closed!");
                                true
                            }
                        }
                    } else {
                        false
                    }
                } else {
                    true
                };

                match &mut proj.state {
                    SupervisorState::Starting(starter) => {
                        match ready!(starter.as_mut().poll_start(cx)) {
                            Ok(Some(supervisee)) => {
                                *proj.state = SupervisorState::Supervising(Box::pin(supervisee));
                            }
                            Ok(None) => {
                                *proj.state = SupervisorState::Exited;
                                return Poll::Ready(None);
                            }
                            Err(e) => {
                                let mut state = SupervisorState::Exited;
                                mem::swap(&mut state, &mut proj.state);
                                let SupervisorState::Starting(starter) = state else {
                                    unreachable!()
                                };
                                return Poll::Ready(Some(starter));
                            }
                        }
                    }
                    SupervisorState::Supervising(supervisee) => {
                        let starter = match to_shutdown {
                            true => ready!(supervisee.as_mut().poll_supervise(cx)),
                            false => ready!(supervisee.as_mut().poll_shutdown(cx)),
                        };
                        *proj.state = SupervisorState::Exited;
                        return Poll::Ready(starter.map(|starter| Box::pin(starter)));
                    }
                    SupervisorState::Exited => panic!("Already exited."),
                }
            }
        }
    }
}

pub trait Supervisable: Debug {
    /// Supervise an actor that has been started.
    ///
    /// Can return:
    /// - `WantsRestart` -> The supervisee would like to be restarted with `poll_start`.
    /// - `Finished` -> The supervisee has exited and does not need to be restarted.
    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<SuperviseeExit>;

    /// Starts the actors supervised under this.
    ///
    /// Can return:
    /// - `Finished` -> The supervisee is finished, and thus will not start.
    /// - `CouldNotStart(_)` -> The supervisee failed to start due to an error.
    /// - `Started` -> The supervisee has been started successfully.
    fn poll_start(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<SuperviseeStart>;

    /// Can return:
    /// - `WantsRestart` -> The supervisee would like to be restarted with `poll_start`.
    /// - `Finished` -> The supervisee has exited and does not need to be restarted.
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<SuperviseeExit>;

    /// Aborts all actors.
    fn abort(self: Pin<&mut Self>);

    /// Whether at least one actor is alive.
    fn is_alive(self: Pin<&Self>) -> bool;
}

pub enum SuperviseeStart {
    Finished,
    CouldNotStart(BoxError),
    Started,
}

pub enum SuperviseeExit {
    WantsRestart,
    Finished,
}

type BoxError = Box<dyn Error + Send>;
type Supervisee = Pin<Box<dyn Supervisable + Send>>;

pub enum SupervisionExit {
    Restart,
    Finished,
}

pub enum SupervisableItem {
    CouldNotStart(BoxError),
    StartedSuccessfully,
    RestartMePlease,
    ImFinished,
}

//------------------------------------------------------------------------------------------------
//  Version 2
//------------------------------------------------------------------------------------------------

/// This is a group of supervisees, where every supervisee will get restarted if they do not return an error.
/// If restarting fails with an error, all supervisees in the group are shut down, and the whole group will
/// be restarted.
#[pin_project]
#[derive(Debug)]
pub struct SuperviseeGroup {
    supervisees: Vec<Supervisee>,
    state: SuperviseeGroupState,
}

#[derive(Debug)]
pub enum SuperviseeGroupState {
    SupervisionShuttingDown(Vec<BoxError>),
    ReadyForSupervision,
    ReadyForStart,
    StartCanceling(Vec<BoxError>),
}

#[derive(thiserror::Error, Debug)]
#[error("{:?}", 0)]
struct ErrorGroup(Vec<BoxError>);

impl Supervisable for SuperviseeGroup {
    fn poll_supervise(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<SuperviseeExit> {
        'outer: loop {
            match &self.state {
                SuperviseeGroupState::ReadyForSupervision => {
                    let proj = self.as_mut().project();

                    let mut start_errors = Vec::new();
                    proj.supervisees.retain_mut(|supervisee| 'retain: loop {
                        if supervisee.as_ref().is_alive() {
                            match supervisee.as_mut().poll_supervise(cx) {
                                Poll::Ready(SuperviseeExit::Finished) => break 'retain false,
                                Poll::Ready(SuperviseeExit::WantsRestart) => (),
                                Poll::Pending => break 'retain true,
                            }
                        } else {
                            match supervisee.as_mut().poll_start(cx) {
                                Poll::Ready(SuperviseeStart::Finished) => break 'retain false,
                                Poll::Ready(SuperviseeStart::CouldNotStart(e)) => {
                                    start_errors.push(e);
                                    break 'retain true;
                                }
                                Poll::Ready(SuperviseeStart::Started) | Poll::Pending => {
                                    break 'retain true
                                }
                            }
                        }
                    });

                    if proj.supervisees.is_empty() {
                        *proj.state = SuperviseeGroupState::ReadyForStart;
                        break 'outer Poll::Ready(SuperviseeExit::Finished);
                    } else if !start_errors.is_empty() {
                        *proj.state = SuperviseeGroupState::SupervisionShuttingDown(start_errors);
                    } else {
                        break 'outer Poll::Pending;
                    }
                }
                SuperviseeGroupState::SupervisionShuttingDown(_) => {
                    let shutdown_result = ready!(self.as_mut().poll_shutdown(cx));
                    self.state = SuperviseeGroupState::ReadyForStart;
                    break 'outer Poll::Ready(shutdown_result);
                }
                SuperviseeGroupState::ReadyForStart => panic!(),
                SuperviseeGroupState::StartCanceling(_) => panic!(),
            }
        }
    }

    fn poll_start(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<SuperviseeStart> {
        'outer: loop {
            match &self.state {
                SuperviseeGroupState::ReadyForStart => {
                    let proj = self.as_mut().project();

                    let mut all_supervisees_have_started = true;

                    let mut start_errors = Vec::new();
                    proj.supervisees.retain_mut(|supervisee| {
                        if let Poll::Ready(res) = supervisee.as_mut().poll_start(cx) {
                            match res {
                                SuperviseeStart::Finished => return false,
                                SuperviseeStart::CouldNotStart(e) => {
                                    start_errors.push(e);
                                }
                                SuperviseeStart::Started => {}
                            }
                        } else {
                            all_supervisees_have_started = false;
                        }
                        true
                    });

                    if proj.supervisees.is_empty() {
                        *proj.state = SuperviseeGroupState::ReadyForSupervision;
                        break 'outer Poll::Ready(SuperviseeStart::Finished);
                    } else if !start_errors.is_empty() {
                        *proj.state = SuperviseeGroupState::StartCanceling(start_errors);
                    } else if all_supervisees_have_started {
                        *proj.state = SuperviseeGroupState::ReadyForSupervision;
                        break 'outer Poll::Ready(SuperviseeStart::Started);
                    } else {
                        break 'outer Poll::Pending;
                    }
                }
                SuperviseeGroupState::StartCanceling(_) => {
                    let _ = ready!(self.as_mut().poll_shutdown(cx));

                    // set state as `ReadyToStart`
                    let mut state = SuperviseeGroupState::ReadyForStart;
                    mem::swap(&mut state, &mut self.state);
                    let SuperviseeGroupState::StartCanceling(errors) = state else {
                        unreachable!()
                    };

                    break 'outer Poll::Ready(SuperviseeStart::CouldNotStart(Box::new(
                        ErrorGroup(errors),
                    )));
                }
                SuperviseeGroupState::SupervisionShuttingDown(_) => panic!(),
                SuperviseeGroupState::ReadyForSupervision => panic!(),
            }
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<SuperviseeExit> {
        let proj = self.project();

        let mut finished_idxs = Vec::new();
        proj.supervisees
            .iter_mut()
            .enumerate()
            .filter(|(_, supervisee)| supervisee.as_ref().is_alive())
            .for_each(|(idx, supervisee)| {
                if let Poll::Ready(res) = supervisee.as_mut().poll_shutdown(cx) {
                    match res {
                        SuperviseeExit::WantsRestart => (),
                        SuperviseeExit::Finished => finished_idxs.push(idx),
                    }
                }
            });

        for idx in finished_idxs.into_iter().rev() {
            proj.supervisees.swap_remove(idx);
        }

        if proj.supervisees.is_empty() {
            Poll::Ready(SuperviseeExit::Finished)
        } else {
            Poll::Ready(SuperviseeExit::WantsRestart)
        }
    }

    fn abort(mut self: Pin<&mut Self>) {
        self.supervisees
            .iter_mut()
            .for_each(|supervisee| supervisee.as_mut().abort())
    }

    fn is_alive(self: Pin<&Self>) -> bool {
        self.supervisees
            .iter()
            .find(|supervisee| supervisee.as_ref().is_alive())
            .is_some()
    }
}

#[derive(Debug)]
pub struct MySupervisee {
    child: Option<Child<(), Inbox<()>>>,
}

impl Supervisable for MySupervisee {
    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<SuperviseeExit> {
        todo!()
    }

    fn poll_start(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<SuperviseeStart> {
        todo!()
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<SuperviseeExit> {
        if let Some(child) = &mut self.child {
            // ready!(child.shutdown(Duration::from_secs(1)).poll_unpin(cx));
            todo!()
        } else {
            Poll::Ready(SuperviseeExit::WantsRestart)
        }
    }

    fn abort(mut self: Pin<&mut Self>) {
        if let Some(child) = &mut self.child {
            child.abort();
        }
    }

    fn is_alive(self: Pin<&Self>) -> bool {
        if let Some(child) = &self.child {
            !child.is_finished()
        } else {
            false
        }
    }
}
