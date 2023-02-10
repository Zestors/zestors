use super::*;
use futures::Future;
use pin_project::pin_project;
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

//------------------------------------------------------------------------------------------------
//  Specification
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct OneForOneSpec<S: Specification = DynSpec> {
    specs: Vec<S>,
    limiter: RestartLimiter,
}

impl<S: Specification> OneForOneSpec<S> {
    pub fn new(limit: usize, within: Duration) -> Self {
        Self {
            specs: Vec::new(),
            limiter: RestartLimiter::new(limit, within),
        }
    }

    pub fn with_spec(mut self, spec: S) -> Self {
        self.add_spec(spec);
        self
    }

    pub fn add_spec(&mut self, spec: S) {
        self.specs.push(spec)
    }
}

impl<S> Specification for OneForOneSpec<S>
where
    S: Specification,
    S::Fut: Unpin,
    S::Supervisee: Unpin,
{
    type Ref = Vec<S::Ref>;
    type Supervisee = OneForOneSupervisee<S>;
    type Fut = OneForOneSpecFut<S>;

    fn start(self) -> Self::Fut {
        OneForOneSpecFut {
            items: self
                .specs
                .into_iter()
                .map(|spec| SpecStateWith::Starting(spec.start()))
                .collect(),
            limiter: Some(self.limiter),
            state: StartFutState::Starting,
        }
    }

    fn start_timeout(&self) -> Duration {
        self.specs
            .iter()
            .fold(Duration::ZERO, |duration, spec| {
                Ord::max(spec.start_timeout(), duration)
            })
            .saturating_add(Duration::from_millis(10))
    }
}

//------------------------------------------------------------------------------------------------
//  Future
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct OneForOneSpecFut<Sp: Specification = DynSpec> {
    items: Vec<SpecStateWith<Sp>>,
    limiter: Option<RestartLimiter>,
    state: StartFutState,
}

impl<Sp: Specification> OneForOneSpecFut<Sp> {
    /// # Panics
    /// Panics if there is still a spec starting.
    fn start_was_successful(&self) -> bool {
        self.items
            .iter()
            .find(|item| match item {
                SpecStateWith::Starting(_) => panic!("Not done starting!"),
                SpecStateWith::Dead(_) => true,
                SpecStateWith::Unhandled(_) => true,
                SpecStateWith::Alive(_, _) => false,
                SpecStateWith::Finished => false,
            })
            .is_none()
    }
}

enum StartFutState {
    Starting,
    ShuttingDown,
}

impl<S> Future for OneForOneSpecFut<S>
where
    S: Specification,
    S::Fut: Unpin,
    S::Supervisee: Unpin,
{
    type Output = StartResult<OneForOneSpec<S>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        'outer: loop {
            match this.state {
                StartFutState::Starting => {
                    let mut started = true;

                    for item in &mut *this.items {
                        if let SpecStateWith::Starting(spec) = item {
                            started = false;

                            if let Poll::Ready(res) = Pin::new(spec).poll(cx) {
                                match res {
                                    Err(StartError::Finished) => *item = SpecStateWith::Finished,
                                    Err(StartError::Failure(spec)) => {
                                        if this.limiter.as_mut().unwrap().restart_within_limit() {
                                            *item = SpecStateWith::Starting(spec.start());
                                            todo!("poll fut here");
                                        } else {
                                            *item = SpecStateWith::Dead(spec)
                                        }
                                    }
                                    Ok((supervisee, reference)) => {
                                        *item = SpecStateWith::Alive(supervisee, reference)
                                    }
                                    Err(StartError::Unhandled(error)) => {
                                        *item = SpecStateWith::Unhandled(error)
                                    }
                                }
                            }
                        }
                    }

                    if started {
                        if this.start_was_successful() {
                            let mut items = Vec::new();
                            std::mem::swap(&mut items, &mut this.items);

                            let mut refs = Vec::new();
                            let supervisees = items.into_iter().filter_map(|item| match item {
                                SpecStateWith::Alive(supervisee, reference) => {
                                    refs.push(reference);
                                    Some(supervisee)
                                }
                                SpecStateWith::Finished => None,
                                SpecStateWith::Starting(_) => panic!(),
                                SpecStateWith::Dead(_) => panic!(),
                                SpecStateWith::Unhandled(_) => panic!(),
                            });

                            break 'outer Poll::Ready(Ok((
                                OneForOneSupervisee::new(supervisees, this.limiter.take().unwrap()),
                                refs,
                            )));
                        } else {
                            this.state = StartFutState::ShuttingDown
                        };
                    } else {
                        break 'outer Poll::Pending;
                    }
                }
                StartFutState::ShuttingDown => {
                    for item in &mut *this.items {
                        match item {
                            SpecStateWith::Starting(_) => panic!("Already finished"),
                            SpecStateWith::Alive(_, _) => todo!(),
                            SpecStateWith::Dead(_) => todo!(),
                            SpecStateWith::Unhandled(_) => todo!(),
                            SpecStateWith::Finished => todo!(),
                        }
                    }
                }
            };
        }
    }
}

//------------------------------------------------------------------------------------------------
//  Supervisee
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct OneForOneSupervisee<S: Specification = DynSpec> {
    items: Vec<SuperviseeItem<S::Supervisee>>,
    limiter: RestartLimiter,
    state: SuperviseeState,
}

enum SuperviseeItem<S: Supervisee> {
    Alive(S),
    Restarting(S::Spec),
    Unrecoverable(BoxError),
    Finished,
}

#[derive(PartialEq, Eq, Debug)]
enum SuperviseeState {
    Supervising,
    ShuttingDown,
}

impl<S: Specification> OneForOneSupervisee<S> {
    fn new(supervisees: impl Iterator<Item = S::Supervisee>, limiter: RestartLimiter) -> Self {
        Self {
            limiter,
            items: supervisees.map(SuperviseeItem::Alive).collect(),
            state: SuperviseeState::Supervising,
        }
    }
}

impl<S> Supervisee for OneForOneSupervisee<S>
where
    S: Specification,
    S::Fut: Unpin,
    S::Supervisee: Unpin,
{
    type Spec = OneForOneSpec<S>;

    fn halt(mut self: Pin<&mut Self>) {
        self.items.iter_mut().for_each(|item| {
            if let SuperviseeItem::Alive(supervisee) = item {
                Pin::new(supervisee).halt()
            }
        })
    }

    fn abort(mut self: Pin<&mut Self>) {
        self.items.iter_mut().for_each(|item| {
            if let SuperviseeItem::Alive(supervisee) = item {
                Pin::new(supervisee).abort()
            }
        })
    }

    fn abort_timeout(self: Pin<&Self>) -> Duration {
        self.items
            .iter()
            .fold(Duration::ZERO, |duration, item| {
                if let SuperviseeItem::Alive(supervisee) = item {
                    Ord::max(Pin::new(supervisee).abort_timeout(), duration)
                } else {
                    duration
                }
            })
            .saturating_add(Duration::from_millis(10))
    }
}

impl<S> Future for OneForOneSupervisee<S>
where
    S: Specification,
    S::Fut: Unpin,
    S::Supervisee: Unpin,
{
    type Output = ExitResult<OneForOneSpec<S>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        '_outer: loop {
            match &this.state {
                SuperviseeState::Supervising => {
                    'supervising: for item in &mut *this.items {
                        match item {
                            SuperviseeItem::Alive(supervisee) => {
                                if let Poll::Ready(exit) = Pin::new(supervisee).poll(cx) {
                                    match exit {
                                        Ok(Some(spec)) => {
                                            *item = SuperviseeItem::Restarting(spec);
                                            if !this.limiter.restart_within_limit() {
                                                this.state = SuperviseeState::ShuttingDown;
                                                break 'supervising;
                                            }
                                        }
                                        Ok(None) => *item = SuperviseeItem::Finished,
                                        Err(e) => *item = SuperviseeItem::Unrecoverable(e),
                                    }
                                }
                            }
                            SuperviseeItem::Restarting(spec) => {
                                todo!()
                                // if let Poll::Ready(exit) = Pin::new(spec).poll_start(cx) {
                                //     todo!()
                                // }
                            }
                            SuperviseeItem::Unrecoverable(_) => todo!(),
                            SuperviseeItem::Finished => todo!(),
                        }
                    }

                    if this.state != SuperviseeState::Supervising {
                        break Poll::Pending;
                    }
                }
                SuperviseeState::ShuttingDown => todo!(),
            }
        }
    }
}
