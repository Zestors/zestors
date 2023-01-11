use crate::*;
use futures::{Future, FutureExt, Stream, StreamExt};
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

    pub fn add_spec<S>(&mut self, spec: S)
    where
        S: SpecifiesChild,
    {
        self.child_specs.push(RawStarter::new(
            async move { // Do something before the child is started.
                let (child, restarter) = spec.start().await?;
                // Do something after the child is started
                Ok((child, restarter))
            }, // And we can add a futures here that get triggered based on certain child behavior
        ).into_dyn());
    }

    pub fn add_spec2<S: SpecifiesChild>(mut self, spec: S) -> Self {
        self.child_specs.push(DynamicChildSpec::new(
            spec.map_before_start(|res| async move { res })
                .map_after_start(|res| async move { res }),
        ));
        self
    }

    pub async fn start(self) -> Result<(Child<SupervisorExit>, SupervisorRef), StartError> {
        // let mut children = Vec::with_capacity(self.child_specs.len());
        // Supervisor::start_from(self.child_specs).await
        Supervisor::_start_from(self.child_specs).await
    }
}

impl Startable for SupervisorBuilder {
    type Ok = (Child<SupervisorExit>, SupervisorRef);
    type Fut = BoxStartFut<Self::Ok>;

    fn start(self) -> Self::Fut {
        Box::pin(self.start())
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
    async fn _start_from(
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

// impl StartableFrom<Vec<DynamicChildSpec>> for Supervisor {
//     type Ok = (Child<SupervisorExit>, SupervisorRef);
//     type Fut = BoxStartFut<Self::Ok>;

//     fn start_from(with: Vec<DynamicChildSpec>) -> Self::Fut {
//         Box::pin(Self::_start_from(with))
//     }
// }

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

pub struct SupervisorRef {
    address: Address<SupervisorProtocol>,
}
type SupervisorExit = Result<(), SupervisorError>;
pub enum SupervisorError {
    ChildFailedToStart(StartError),
}
type SupervisorProtocol = ();
