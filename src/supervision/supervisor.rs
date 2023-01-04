use crate::*;
use futures::{future::BoxFuture, pin_mut, Future, FutureExt, Stream, StreamExt};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

//------------------------------------------------------------------------------------------------
//  SupervisorBuilder
//------------------------------------------------------------------------------------------------

pub struct SupervisorBuilder {
    child_specs: Vec<DynamicChildSpec>,
}

impl SupervisorBuilder {
    pub fn new() -> Self {
        Self {
            child_specs: Vec::new(),
        }
    }

    pub fn add_spec<S: SpecifiesChild<S>>(mut self, spec: S) -> Self {
        self.child_specs.push(DynamicChildSpec::new(spec));
        self
    }

    pub async fn start(self) -> Result<(), StartError> {
        let mut children = Vec::with_capacity(self.child_specs.len());

        for spec in self.child_specs {
            let child = spec.start().await?;
            children.push(child);
        }

        Ok(())
    }
}

//------------------------------------------------------------------------------------------------
//  Supervisor
//------------------------------------------------------------------------------------------------

struct Supervisor {
    supervisees: Vec<DynamicSupervisee>,
    inbox: Inbox<SupervisorProtocol>,
}

impl Supervisor {
    async fn start(
        child_specs: Vec<DynamicChildSpec>,
    ) -> Result<(Child<SupervisorExit>, SupervisorRef), StartError> {
        let mut supervisees = Vec::new();
        for spec in child_specs {
            supervisees.push(spec.start().await?);
        }

        let (child, address) = spawn_process(Config::default(), move |inbox| async move {
            Self { supervisees, inbox }.await
        });

        Ok((child.into_dyn(), SupervisorRef { address }))
    }
}

impl StartableWith<Vec<DynamicChildSpec>> for Supervisor {
    type Output = (Child<SupervisorExit>, SupervisorRef);
    type Fut = BoxStartFut<Self::Output>;

    fn start_with(with: Vec<DynamicChildSpec>) -> Self::Fut {
        Box::pin(Self::start(with))
    }
}

impl Unpin for Supervisor {}

impl Future for Supervisor {
    type Output = SupervisorExit;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        fn poll_until_ok(
            supervisee: &mut DynamicSupervisee,
            cx: &mut Context<'_>,
        ) -> Result<bool, StartError> {
            loop {
                match supervisee.next().poll_unpin(cx) {
                    // Wants to be restarted, so we have to poll it again
                    Poll::Ready(Some(Ok(()))) => {}
                    // Error, exit now
                    Poll::Ready(Some(Err(error))) => break Err(error),
                    // Is finished, can be removed
                    Poll::Ready(None) => break Ok(true),
                    // Pending, ok
                    Poll::Pending => break Ok(false),
                }
            }
        }

        if self.supervisees.len() == 0 {
            return Poll::Ready(Ok(()));
        }

        let mut to_remove = Vec::new();

        let result = self
            .supervisees
            .iter_mut()
            .enumerate()
            .find_map(|(i, supervisee)| {
                match poll_until_ok(supervisee, cx) {
                    // is finished
                    Ok(true) => {
                        to_remove.push(i);
                        None
                    }
                    // Not finished yet, pending
                    Ok(false) => None,
                    // Failed to restart, supervisor will now exit
                    Err(e) => {
                        to_remove.push(i);
                        Some(e)
                    }
                }
            });

        for i in to_remove.into_iter().rev() {
            self.supervisees.swap_remove(i);
        }

        match result {
            Some(e) => Poll::Ready(Err(SupervisorError::ChildFailedToStart(e))),
            None => Poll::Pending,
        }
    }
}

struct SupervisorRef {
    address: Address<SupervisorProtocol>,
}
type SupervisorExit = Result<(), SupervisorError>;
enum SupervisorError {
    ChildFailedToStart(StartError),
}
type SupervisorProtocol = ();
