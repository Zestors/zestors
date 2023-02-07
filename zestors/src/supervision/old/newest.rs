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
