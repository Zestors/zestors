use super::*;
use async_trait::async_trait;
use futures::{future::BoxFuture, Future, FutureExt};
use pin_project::pin_project;
use std::{
    fmt::Debug,
    mem::swap,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use thiserror::Error;
use tokio::time::{sleep, Instant, Sleep};

//------------------------------------------------------------------------------------------------
//  Spec
//------------------------------------------------------------------------------------------------

#[pin_project]
#[derive(Debug)]
pub struct OneForOneSpec {
    items: Vec<OneForOneItem>,
    limiter: RestartLimiter,
}

impl OneForOneSpec {
    pub fn new(limit: usize, within: Duration) -> Self {
        Self {
            items: Vec::new(),
            limiter: RestartLimiter::new(limit, within),
        }
    }

    pub fn with_spec<S: Specification>(mut self, spec: S) -> Self
    where
        S: Send + 'static,
        S::Supervisee: Send,
    {
        self.add_spec(spec);
        self
    }

    pub fn add_spec<S: Specification>(&mut self, spec: S)
    where
        S: Send + 'static,
        S::Supervisee: Send,
    {
        self.items
            .push(OneForOneItem::Spec(spec.on_start(|_| ()).into_dyn()))
    }

    pub fn pop_spec(&mut self) -> Option<BoxSpec> {
        loop {
            match self.items.pop() {
                Some(OneForOneItem::Spec(spec)) => break Some(spec),
                None => break None,
                _ => (),
            }
        }
    }
}

#[async_trait]
impl Specification for OneForOneSpec {
    type Ref = ();
    type Supervisee = OneForOneSupervisee;

    async fn start_supervised(mut self) -> StartResult<Self> {
        for item in self.items.iter_mut() {
            item.start().expect("Is a spec");
        }

        OneForOneStartFut {
            spec: Some(self),
            start_failure: false,
            shutdown_timer: None,
        }
        .await
    }
}

//------------------------------------------------------------------------------------------------
//  StartFut
//------------------------------------------------------------------------------------------------

#[pin_project]
struct OneForOneStartFut {
    spec: Option<OneForOneSpec>,
    shutdown_timer: Option<Pin<Box<Sleep>>>,
    start_failure: bool,
}

#[derive(Debug, Error)]
#[error("{}{:?}", 0, 1)]
struct OneForOneError(&'static str, OneForOneSpec);

#[allow(unused_assignments)]
impl OneForOneStartFut {
    fn take_start_now(&mut self) -> StartResult<OneForOneSpec> {
        let inner = self.spec.take().unwrap();

        let mut ok = true;
        let mut irrecoverable = false;
        let mut completed = true;

        for item in &inner.items {
            match item {
                OneForOneItem::StartFut(_) => {
                    ok = false;
                    irrecoverable = true
                }
                OneForOneItem::Supervisee(_, _) => irrecoverable = true,
                OneForOneItem::Irrecoverable(_) => {
                    ok = false;
                    irrecoverable = true
                }
                OneForOneItem::Spec(_) => completed = false,
                OneForOneItem::Completed => (),
            }
        }

        if ok {
            Ok((OneForOneSupervisee::new(inner), ()))
        } else if irrecoverable {
            Err(StartError::Fatal(Box::new(
                OneForOneError("Error: ", inner),
            )))
        } else if completed {
            Err(StartError::Completed)
        } else {
            let items = inner
                .items
                .into_iter()
                .filter(|item| matches!(item, OneForOneItem::Spec(_)))
                .collect::<Vec<_>>();

            Err(StartError::StartFailed(OneForOneSpec {
                items,
                limiter: inner.limiter,
            }))
        }
    }
}

#[allow(unused_labels)]
impl Future for OneForOneStartFut {
    type Output = StartResult<OneForOneSpec>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        let inner = this.spec.as_mut().unwrap();

        'outer: loop {
            if !this.start_failure {
                let mut all_ready = true;

                'inner: for item in &mut inner.items {
                    if let OneForOneItem::StartFut(start_fut) = item {
                        if let Poll::Ready(start_res) = start_fut.poll_unpin(cx) {
                            match start_res {
                                Ok((supervisee, _)) => {
                                    *item = OneForOneItem::Supervisee(supervisee, None);
                                }
                                Err(StartError::Completed) => {
                                    *item = OneForOneItem::Completed
                                }
                                Err(StartError::StartFailed(spec)) => {
                                    *item = OneForOneItem::Spec(spec);
                                    if !inner.limiter.within_limit() {
                                        this.start_failure = true;
                                        break 'inner;
                                    }
                                }
                                Err(StartError::Fatal(e)) => {
                                    *item = OneForOneItem::Irrecoverable(e);
                                    if !inner.limiter.within_limit() {
                                        this.start_failure = true;
                                        break 'inner;
                                    }
                                }
                            }
                        } else {
                            all_ready = false;
                        }
                    }
                }

                if this.start_failure {
                    // Reset the timeout because we are now going to shut everything down.
                    this.shutdown_timer = Some(Box::pin(sleep(todo!())));
                } else if all_ready {
                    let supervisee = OneForOneSupervisee::new(this.spec.take().unwrap());
                    break 'outer Poll::Ready(Ok((supervisee, ())));
                } else {
                    break 'outer Poll::Pending;
                };
            } else {
                let mut all_ready = true;

                'inner: for item in &mut inner.items {
                    match item {
                        OneForOneItem::StartFut(fut) => {
                            if let Poll::Ready(start_res) = fut.poll_unpin(cx) {
                                match start_res {
                                    Ok((supervisee, _)) => {
                                        *item = OneForOneItem::Supervisee(supervisee, None);
                                        all_ready = false;
                                    }
                                    Err(StartError::Completed) => {
                                        *item = OneForOneItem::Completed
                                    }
                                    Err(StartError::StartFailed(spec)) => {
                                        *item = OneForOneItem::Spec(spec);
                                    }
                                    Err(StartError::Fatal(e)) => {
                                        *item = OneForOneItem::Irrecoverable(e);
                                    }
                                }
                            } else {
                                all_ready = false;
                            }
                        }
                        OneForOneItem::Supervisee(supervisee, _) => {
                            if let Poll::Ready(exit_res) = Pin::new(supervisee).poll_supervise(cx) {
                                match exit_res {
                                    Ok(Some(spec)) => {
                                        *item = OneForOneItem::Spec(spec);
                                    }
                                    Ok(None) => {
                                        *item = OneForOneItem::Completed;
                                    }
                                    Err(e) => {
                                        *item = OneForOneItem::Irrecoverable(e);
                                    }
                                }
                            } else {
                                all_ready = false;
                            }
                        }
                        _ => (),
                    }
                }

                if all_ready {
                    break 'outer Poll::Ready(this.take_start_now());
                } else {
                    break 'outer Poll::Pending;
                };
            }
        }
    }
}

#[pin_project]
pub struct OneForOneSupervisee {
    inner: Option<OneForOneSpec>,
    halted: bool,
    aborted: bool,
}

impl OneForOneSupervisee {
    fn new(inner: OneForOneSpec) -> Self {
        Self {
            inner: Some(inner),
            halted: false,
            aborted: false,
        }
    }
}

impl Supervisee for OneForOneSupervisee {
    type Spec = OneForOneSpec;

    fn shutdown_time(self: Pin<&Self>) -> Duration {
        Duration::MAX.into()
    }

    fn halt(mut self: Pin<&mut Self>) {
        self.halted = true;

        for item in &mut self.inner.as_mut().unwrap().items {
            if let OneForOneItem::Supervisee(supervisee, _) = item {
                Pin::new(supervisee).halt()
            }
        }
    }

    fn abort(mut self: Pin<&mut Self>) {
        self.aborted = true;

        for item in &mut self.inner.as_mut().unwrap().items {
            if let OneForOneItem::Supervisee(supervisee, _) = item {
                Pin::new(supervisee).abort()
            }
        }
    }

    fn poll_supervise(
        self: Pin<&mut Self>,
        _cx: &mut Context,
    ) -> Poll<SupervisionResult<Self::Spec>> {
        todo!()
    }
}

//------------------------------------------------------------------------------------------------
//  Item
//------------------------------------------------------------------------------------------------

enum OneForOneItem {
    Spec(BoxSpec),
    StartFut(BoxFuture<'static, StartResult<BoxSpec>>),
    Supervisee(BoxSupervisee, Option<Instant>),
    Irrecoverable(FatalError),
    Completed,
}

impl Debug for OneForOneItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spec(arg0) => f.debug_tuple("Spec").field(arg0).finish(),
            Self::StartFut(arg0) => f.debug_tuple("StartFut").field(&"..").finish(),
            Self::Supervisee(arg0, arg1) => {
                f.debug_tuple("Supervisee").field(arg0).field(arg1).finish()
            }
            Self::Irrecoverable(arg0) => f.debug_tuple("Irrecoverable").field(arg0).finish(),
            Self::Completed => write!(f, "Completed"),
        }
    }
}

impl OneForOneItem {
    fn start(&mut self) -> Result<(), Box<dyn Error>> {
        let spec = {
            let mut item = OneForOneItem::Completed;
            swap(self, &mut item);
            let Self::Spec(spec) = item else {
                return Err(format!("{:?} was not a spec", item).into());
            };
            spec
        };

        *self = OneForOneItem::StartFut(spec.start_supervised());
        Ok(())
    }
}
