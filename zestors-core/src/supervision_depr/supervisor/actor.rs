use std::{
    future::poll_fn,
    pin::{Pin, pin},
    task::{self, Poll},
};

use crate::{
    _prelude::*,
    supervision_depr::supervisor::{Supervisee, SuperviseeEvent},
};
use futures::{FutureExt as _, Stream, future::join_all, ready, stream::StreamExt as _};
use indexmap::{IndexMap, IndexSet};
use rootcause::prelude::{IteratorExt as _, ResultExt as _};

#[derive(Debug)]
pub struct Supervisor {
    supervisees: IndexMap<Pid, (Supervisee, ChildDescription)>,
    strategy: SupervisionStrategy,
    restart_limiter: RestartLimiter,
    source: Option<Box<dyn SupervisorSource>>,
    is_initialized: bool,
}

impl Supervisor {
    /// Loads the list of dynamic/static supervisees currently managed by the
    /// supervisor, and starts them.
    ///
    /// This does not mean that the supervisees have completed their initialization,
    /// only that they have been started.
    async fn start(&mut self, inbox: &mut Inbox<SupervisorInterface>) -> Result<(), Report> {
        inbox
            .run_until_shutdown(async {
                // First, load all dynamic supervisees from the source, if any.
                if let Some(source) = &mut self.source {
                    let dyn_children = source
                        .load_all()
                        .await
                        .attach("Failed to read dynamic supervisees")?;

                    // Add all dynamic supervisees to the supervisor's supervisee list
                    for child in dyn_children {
                        let pid = child.pid().clone();
                        let descr = ChildDescription {
                            pid: pid.clone(),
                            cfg: child.cfg().clone(),
                        };
                        let supervisee = Supervisee::new_dynamic(child);
                        self.supervisees.insert(pid, (supervisee, descr));
                    }
                }

                // Start starting all supervisees
                for supervisee in self.supervisees.values_mut() {
                    supervisee.0.start();
                }

                Ok::<(), Report>(())
            })
            .await??;

        Ok(())
    }

    pub fn watch_initialization(
        &mut self,
    ) -> impl Future<Output = Result<(), Report>> + Send + 'static {
        let initialization_addresses = self
            .supervisees
            .values()
            .map(|(s, _)| s.address().clone())
            .collect::<Vec<_>>();

        async move {
            join_all(
                initialization_addresses
                    .into_iter()
                    .map(|addr| async move { (addr.watch_initialization().await, addr) }),
            )
            .await
            .into_iter()
            .map(|(res, addr)| match res {
                Ok(()) => Ok(()),
                Err(exit) => exit
                    .into_result()
                    .attach(format!("Child {} failed to initialize", addr.pid())),
            })
            .collect_reports()
            .map_err(|e| report!("One or more children failed to start").attach(e))
        }
    }
}

impl Actor for Supervisor {
    type Interface = SupervisorInterface;
    type Exit = ();

    async fn run(mut self, mut inbox: Inbox<Self::Interface>) -> Result<Self::Exit, Report> {
        if let Err(e) = self.start(&mut inbox).await {
            return Err(self.shutdown_all_with_err(&mut inbox, e).await);
        }

        tracing::info!("Supervisor started successfully");

        let mut watch_init = pin!(self.watch_initialization());

        loop {
            let msg = tokio::select! {
                biased;

                // Watches for initialization of all initial supervisees.
                // Once all supervisees have initialized, is_initialized is set to true
                // This branch only runs once, and is skipped after all supervisees have initialized.
                init_result = &mut watch_init, if !self.is_initialized => {
                    if let Err(e) = init_result {
                        return Err(self.shutdown_all_with_err(&mut inbox, e).await);
                    }

                    self.is_initialized = true;
                    tracing::info!("All supervisees have completed initialization");
                    continue;
                }

                // Watches for messages from supervisees, and handles them accordingly
                // Only sets `init` once all initial supervisees have completed initialization
                Some(msg) = inbox.next_with_init(self.is_initialized) => {
                    msg
                }

                Some((pid, item)) = next_supervisee_event(&mut self.supervisees) => {
                    if let Err(e) = self.handle_supervisee_event(pid, item, &mut inbox).await {
                        return Err(self.shutdown_all_with_err(&mut inbox, e).await);
                    }
                    continue;
                }

                Some(source_event) = next_source_event(&self.source) => {
                    if let Err(e) = self.handle_source_event(source_event, &mut inbox).await {
                        return Err(self.shutdown_all_with_err(&mut inbox, e).await);
                    }
                    continue;
                }

                _ = futures::future::pending::<()>() => {
                    unreachable!()
                }
            };

            match msg {
                Event::Signal(signal) => match signal {
                    Signal::Shutdown => {
                        return self.shutdown_all(&mut inbox).await;
                    }
                    Signal::Resume | Signal::Suspend => (),
                },
                Event::Message(msg) => self.handle_msg(msg).await,
            }
        }

        Ok(())
    }
}

impl Supervisor {
    async fn register_child(&mut self, spec: ChildSpec) -> Result<(), DuplicatePidError> {
        if self.supervisees.contains_key(spec.pid()) {
            return Err(DuplicatePidError {
                pid: spec.pid().clone(),
            });
        }

        if let Some(source) = &self.source {
            source.add(spec.clone()).await.ok();
        }

        let mut supervisee = Supervisee::new_dynamic(spec);
        supervisee.start();
        let pid = supervisee.pid().clone();
        let descr = ChildDescription {
            pid: pid.clone(),
            cfg: supervisee.cfg().clone(),
        };

        self.supervisees
            .insert(supervisee.pid().clone(), (supervisee, descr));

        Ok(())
    }

    async fn deregister_child(&mut self, pid: &Pid) -> Option<Supervisee> {
        if let Some(source) = &self.source {
            source.remove(pid.clone()).await.ok();
        }

        if let Some((supervisee, _)) = self.supervisees.shift_remove(pid) {
            Some(supervisee)
        } else {
            None
        }
    }

    async fn handle_msg(&mut self, msg: SupervisorInterface) {
        match msg {
            // SupervisorInterface::RegisterChild(Envelope {
            //     msg: RegisterChild(spec),
            //     handle,
            // }) => {
            //     let result = self.register_child(spec).await;
            //     handle.send(result).ok();
            // }

            // SupervisorInterface::DeregisterChild(Envelope {
            //     msg: DeregisterChild(id),
            //     handle,
            // }) => {
            //     let Some(mut supervisee) = self.deregister_child(&id).await else {
            //         handle.send(None).ok();
            //         return;
            //     };

            //     let spec = supervisee.spec().clone();

            //     tokio::task::spawn(async move {
            //         supervisee.initiate_shutdown();
            //     });

            //     handle.send(Some(spec)).ok();
            // }
            SupervisorInterface::Children(Envelope { msg: _, handle }) => {
                let children = self.get_child_descriptions();
                handle.send(children).ok();
            }

            SupervisorInterface::Health(Envelope { msg: _, handle }) => {
                // TODO: Check for recent restarts and determine health status accordingly
                handle.send(HealthStatus::Healthy.into_health()).ok();
            }
        }
    }

    pub(super) fn new(
        supervisees: IndexMap<Pid, ChildSpec>,
        strategy: SupervisionStrategy,
        restart_intensity: RestartIntensity,
        source: Option<Box<dyn SupervisorSource>>,
    ) -> Self {
        Self {
            supervisees: supervisees
                .into_iter()
                .map(|(pid, spec)| {
                    let descr = ChildDescription {
                        pid: pid.clone(),
                        cfg: spec.cfg().clone(),
                    };
                    (pid, (Supervisee::new_static(spec), descr))
                })
                .collect(),
            strategy,
            restart_limiter: RestartLimiter::new(restart_intensity),
            is_initialized: false,
            source,
        }
    }

    pub fn blueprint() -> SupervisorBlueprint {
        SupervisorBlueprint::new()
    }

    async fn shutdown_all_with_err(
        &mut self,
        inbox: &mut Inbox<SupervisorInterface>,
        e: impl Into<Report>,
    ) -> Report {
        let pids = self.supervisees.keys().cloned().collect::<Vec<_>>();

        match self.shutdown(&pids, inbox).await {
            Ok(_) => e.into(),
            Err(e2) => e.into().context(e2).into(),
        }
    }

    async fn shutdown_all(&mut self, inbox: &mut Inbox<SupervisorInterface>) -> Result<(), Report> {
        let pids = self.supervisees.keys().cloned().collect::<Vec<_>>();
        self.shutdown(&pids, inbox).await
    }

    async fn shutdown(
        &mut self,
        pids: &[Pid],
        _inbox: &mut Inbox<SupervisorInterface>,
    ) -> Result<(), Report> {
        join_all(
            self.supervisees
                .values_mut()
                .filter(|(s, _)| pids.contains(s.pid()))
                .filter_map(|(s, _)| s.initiate_shutdown())
                .collect::<Vec<_>>(),
        )
        .await
        .into_iter()
        .collect_reports()
        .map_err(|e| e.context("One or more supervisees failed to shutdown cleanly"))
        .map_err(Into::into)
    }

    async fn handle_supervisee_event(
        &mut self,
        pid: Pid,
        event: SuperviseeEvent,
        inbox: &mut Inbox<SupervisorInterface>,
    ) -> Result<(), RestartLimitReached> {
        let supervisee = &self
            .supervisees
            .get(&pid)
            .expect("child should be present")
            .0;
        let result = event.into_result();

        // Restart is required: Start the supervisee again.
        if let (RestartMode::Always, _) | (RestartMode::OnError, Err(_)) =
            (supervisee.cfg().restart_mode, &result)
        {
            if !self.allow_restart() {
                return Err(RestartLimitReached {
                    id: pid.clone(),
                    error: result.err(),
                });
            }

            self.restart_affected_children(&pid, inbox).await;
        }
        // No restart is required: Remove the supervisee from the supervisor's list if it is dynamic.
        else {
            if supervisee.is_dynamic() {
                remove_spec_from_source(
                    &**self.source.as_ref().expect("source should be present"),
                    &pid,
                )
                .await
                .ok();
            }
        }

        Ok(())
    }

    async fn restart_affected_children<'a>(
        &mut self,
        pid: &Pid,
        inbox: &mut Inbox<SupervisorInterface>,
    ) {
        let affected_pids = self.affected_pids(pid);

        _ = self.shutdown(&affected_pids, inbox).await;

        let supervisees = self
            .supervisees
            .iter_mut()
            .filter(|(id, _)| affected_pids.contains(id))
            .map(|(_, supervisee)| supervisee)
            .collect::<Vec<_>>();

        // Now restart all affected children
        for (supervisee, _) in supervisees {
            supervisee.start();
        }
    }

    fn allow_restart(&mut self) -> bool {
        self.restart_limiter.allow_restart()
    }

    fn affected_pids(&self, id: &Pid) -> Vec<Pid> {
        match self.strategy {
            SupervisionStrategy::OneForOne => vec![id.clone()],
            SupervisionStrategy::OneForAll => self.supervisees.keys().cloned().collect(),
            SupervisionStrategy::RestForOne => {
                let mut affected = Vec::new();
                let mut found = false;

                for child_id in self.supervisees.keys() {
                    if child_id == id {
                        found = true;
                    }

                    if found {
                        affected.push(child_id.clone());
                    }
                }

                affected
            }
        }
    }

    fn get_child_descriptions(&self) -> Vec<ChildDescription> {
        self.supervisees
            .iter()
            .map(|(pid, (_, desc))| desc.clone())
            .collect()
    }

    async fn handle_source_event(
        &mut self,
        event: SupervisorSourceEvent,
        _inbox: &mut Inbox<SupervisorInterface>,
    ) -> Result<(), Report> {
        // match event {
        //     SupervisorSourceEvent::Added(spec) => {
        //         tracing::info!("Dynamic supervisee {} added to source", spec.pid());

        //         let pid = spec.pid().clone();
        //         self.supervisees
        //             .insert(pid, Supervisee::new_dynamic(spec, true));
        //     }

        //     SupervisorSourceEvent::Removed(pid) => {
        //         tracing::info!("Dynamic supervisee {} removed from source", pid);

        //         if let Some(supervisee) = self.supervisees.get_mut(&pid) {
        //             let timeout = supervisee.spec.cfg().abort_timeout;
        //             if let Some(child) = &mut supervisee.child_mut() {
        //                 child.signal_shutdown();
        //                 child.abort_after(timeout);
        //             }
        //         } else {
        //             tracing::warn!(
        //                 "Dynamic supervisee {} was not found in supervisor when attempting to remove it",
        //                 pid
        //             );
        //         }
        //     }
        // }
        todo!();

        Ok(())
    }
}

async fn remove_spec_from_source(source: &dyn SupervisorSource, pid: &Pid) -> Result<(), Report> {
    match source.remove(pid.clone()).await {
        Ok(Some(_spec)) => {
            tracing::debug!("Dynamic supervisee {} removed from source", pid);
            Ok(())
        }
        Ok(None) => {
            tracing::warn!(
                "Dynamic supervisee {} was not found in source when attempting to remove it",
                pid
            );
            Ok(())
        }
        Err(e) => {
            tracing::error!(
                error = ?e,
                "Failed to remove dynamic supervisee {} from source",
                pid
            );
            Err(e.attach(format!(
                "Failed to remove dynamic supervisee {} from source",
                pid
            )))
        }
    }
}

async fn next_source_event(
    source: &Option<Box<dyn SupervisorSource>>,
) -> Option<SupervisorSourceEvent> {
    if let Some(source) = source {
        source.next().await
    } else {
        None
    }
}

async fn next_supervisee_event(
    supervisees: &mut IndexMap<Pid, (Supervisee, ChildDescription)>,
) -> Option<(Pid, SuperviseeEvent)> {
    poll_fn(|cx| {
        for (pid, (supervisee, _)) in supervisees.iter_mut() {
            match supervisee.poll_next_unpin(cx) {
                Poll::Ready(Some(event)) => {
                    return Poll::Ready(Some((pid.clone(), event)));
                }
                Poll::Ready(None) => {
                    todo!("Currently not reachable")
                }
                Poll::Pending => (),
            }
        }

        // If no active children remain, return None immediately??
        Poll::Ready(None)
    })
    .await
}

impl Unpin for Supervisor {}
