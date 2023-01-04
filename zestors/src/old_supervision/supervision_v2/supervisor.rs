use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::{pin_mut, FutureExt, Stream};

use crate::*;

pub enum SupervisorStrategy {
    OneForOne,
    OneForAll,
    RestForOne,
}

impl Default for SupervisorStrategy {
    fn default() -> Self {
        Self::OneForOne
    }
}

pub struct SupervisorBuilder {
    pub children: Vec<DynamicChildSpec>,
    pub strategy: SupervisorStrategy,
    pub max_restarts: u32,
    pub max_duration: Duration,
}

impl SupervisorBuilder {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            strategy: SupervisorStrategy::default(),
            max_restarts: 3,
            max_duration: Duration::from_secs(5),
        }
    }

    pub fn add_child<S: Into<DynamicChildSpec>>(&mut self, spec: S) {
        self.children.push(spec.into())
    }

    pub fn set_strategy(mut self, strategy: SupervisorStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn set_max_restarts(mut self, max_restarts: u32) -> Self {
        self.max_restarts = max_restarts;
        self
    }

    pub fn set_max_duration(mut self, max_duration: Duration) -> Self {
        self.max_duration = max_duration;
        self
    }

    pub async fn start(self) -> Result<(Child<SupervisorExit>, SupervisorRef), StartError> {
        Supervisor::start(self).await
    }
}

struct Supervisor {
    children: Vec<DynamicSupervisedChild>,
    inbox: Inbox<SupervisorProtocol>,
    strategy: SupervisorStrategy,
    max_restarts: u32,
    max_duration: Duration,
}

impl Supervisor {
    pub async fn start(
        builder: SupervisorBuilder,
    ) -> Result<(Child<SupervisorExit>, SupervisorRef), StartError> {
        // Start every child one at a time in chronological order.
        // If it returns an error, then pass this error on.
        let mut children = Vec::with_capacity(builder.children.len());
        for child in builder.children {
            children.push(child.start().await?);
        }

        // Now we can spawn and run the supervisor.
        let (child, address) = spawn_process(Config::default(), move |inbox| async move {
            Self {
                children,
                inbox,
                strategy: builder.strategy,
                max_restarts: builder.max_restarts,
                max_duration: builder.max_duration,
            }
            .run()
            .await
        });

        // And return the child and reference
        Ok((child.into_dyn(), SupervisorRef { address }))
    }

    async fn run(self) -> SupervisorExit {}
}

struct SupervisedChildren {
    children: Vec<(DynamicSupervisedChild, SupervisedChildState)>,
}

pub enum SupervisedChildOption {
    Alive(DynamicSupervisedChild),
    Restarting()
}

// impl Unpin for SupervisedChildren {}

// impl Stream for SupervisedChildren {
//     type Item = usize;

//     fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//         if self.children.len() == 0 {
//             return Poll::Ready(None);
//         }

//         for (i, child) in self.children.iter_mut().enumerate() {
//             if let Poll::Ready(()) = child.0.supervise().poll_unpin(cx) {
//                 return Poll::Ready(Some(i));
//             }
//         }

//         Poll::Pending
//     }
// }

enum SupervisedChildState {
    Alive,
    Restarting,
}

enum SupervisionItem {
    // A child has exited, it does not know if it wants to be restarted yet.
    HasExited(usize),
    // A child has failed to start.
    RestartFailed(StartError, usize),
    // A child has exited and would like to restart.
    Restarted(usize),
    // A child has exited and is finished.
    Finished(usize),
}

pub struct SupervisorRef {
    address: Address<SupervisorProtocol>,
}

type SupervisorProtocol = ();
pub type SupervisorExit = ();
