use crate::{self as zestors, *};
use futures::{future, Future, FutureExt};
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

type BoxedChildSpec = Box<dyn FnOnce() -> Box<dyn Supervisable + Send> + Send>;

pub struct SupervisorRef {
    address: Address<()>,
}

pub struct SupervisorBuilder {
    children: Vec<BoxedChildSpec>,
    strategy: SupervisionStrategy,
}

pub enum SupervisionStrategy {
    /// If a child terminates, only that child is restarted
    OneForOne { max: u32, within: Duration },
    /// If a child terminates, that process and all those started after it
    /// are restarted.
    RestForOne { max: u32, within: Duration },
    /// If a child terminates, all other child processes are terminated
    /// and then all processes are restarted.
    OneForAll { max: u32, within: Duration },
}

/// When to restart the child
pub enum RestartStrategy {
    NormalOnly,
    PanicOrNormal,
    HaltOrNormal,
    PanicOrHalt,
    Always,
}

impl Default for RestartStrategy {
    fn default() -> Self {
        Self::NormalOnly
    }
}

pub enum ShutdownStrategy {
    Abort,
    HaltThenAbort(Duration),
    Halt,
}

impl SupervisorBuilder {
    pub fn new(
        strategy: SupervisionStrategy,
        shutdown: ShutdownStrategy,
        restart: RestartStrategy,
    ) -> Self {
        Self {
            children: Vec::new(),
            strategy,
        }
    }

    pub fn add_child<S>(mut self, spec: S) -> Self
    where
        S: SpecifiesChild + Send + 'static,
    {
        self.children.push(Box::new(move || spec.spawn()));
        self
    }

    pub fn spawn(self) -> (Child<()>, SupervisorRef) {
        let (child, address) = spawn_process(Config::default(), |inbox: Inbox<()>| async move {
            Supervisor {
                children: self.children.into_iter().map(|child| child()).collect(),
                inbox,
            }
            .await
        });
        (child.into_dyn(), SupervisorRef { address })
    }
}

impl SpecifiesChild for SupervisorBuilder {
    fn spawn(self) -> Box<dyn Supervisable + Send> {
        let spec = ChildSpec {
            run_fn: |inbox: Inbox<()>, i: ()| async move {},
            init: (),
            restart_fn: |exit: Result<(), ExitError>| Some(async move { Ok(()) }),
            config: Config::default(),
        };

        <ChildSpec<_, _, _, _, _> as SpecifiesChild>::spawn(spec)
    }
}

struct Supervisor {
    children: Vec<Box<dyn Supervisable + Send>>,
    inbox: Inbox<()>,
}

impl Unpin for Supervisor {}

impl Future for Supervisor {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.children.len() == 0 {
            return Poll::Ready(());
        }

        // let mut failed_restarts = Vec::new();

        // for (i, child) in self.children.iter_mut().enumerate() {
        //     match child.state() {
        //         ChildState::Alive => {
        //             let exit = child.supervise().poll_unpin(cx);
        //             if let Poll::Ready(exit) = exit {
        //                 if let Poll::Ready(false) = child.restart(exit).poll_unpin(cx) {
        //                     failed_restarts.push(i)
        //                 }
        //             };
        //         }
        //         ChildState::Restarting => {
        //             if let Poll::Ready(false) = child.continue_restart().poll_unpin(cx) {
        //                 failed_restarts.push(i)
        //             }
        //         }
        //         ChildState::Exited => todo!(),
        //         ChildState::Finished => todo!(),
        //     }
        // }

        Poll::Pending
    }
}
