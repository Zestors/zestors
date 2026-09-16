use std::ops::ControlFlow;

use super::*;

pub(super) struct OneForAllSupervisor<'a> {
    inner: &'a mut SupervisorInner,
    initializing: IndexSet<Pid>,
    exiting: IndexSet<Pid>,
    /// Pids currently being stopped as part of an in-progress group restart,
    /// i.e. the ones we're still waiting to hear an exit from.
    restarting: IndexSet<Pid>,
    /// Members of the current cascade (past or present members of `restarting`)
    /// that should be started again once every member has exited.
    cascade_restart: IndexSet<Pid>,
    /// Members of the current cascade that should be dropped, rather than
    /// restarted, once they've exited (configured with `RestartMode::Never`).
    cascade_drop: IndexSet<Pid>,
}

impl<'a> OneForAllSupervisor<'a> {
    pub(super) fn new(inner: &'a mut SupervisorInner) -> Self {
        Self {
            inner,
            initializing: IndexSet::new(),
            exiting: IndexSet::new(),
            restarting: IndexSet::new(),
            cascade_restart: IndexSet::new(),
            cascade_drop: IndexSet::new(),
        }
    }

    pub(super) async fn run(&mut self) -> Result<(), Report> {
        let start_result = self.inner.supervisees.start_all();
        self.initializing = self.inner.supervisees.pids().cloned().collect();

        // If some supervisees failed to start, tear down whichever ones did start
        // before bailing out, instead of leaving them running unsupervised.
        let should_run = match &start_result {
            Ok(()) => true,
            Err(_) => self.shutdown().is_continue(),
        };

        if should_run {
            while let Some(next) = self.inner.next().await {
                if let ControlFlow::Break(_) = self.handle_next(next) {
                    break;
                }
            }
        }

        start_result.map_err(|(pid, e)| report!(e).attach(pid).into())
    }

    fn handle_next(&mut self, next: InnerNext) -> ControlFlow<()> {
        match next {
            InnerNext::Inbox(ev) => match ev {
                InboxEvent::Signal(signal) => {
                    if signal.is_shutdown() {
                        self.shutdown()?;
                    }
                }

                InboxEvent::Message(msg) => match msg {
                    SupervisorInterface::Children(envelope) => {
                        envelope
                            .reply(self.inner.supervisees.child_descriptions())
                            .ok();
                    }
                    SupervisorInterface::Health(envelope) => {
                        envelope.reply(self.inner.health()).ok();
                    }
                    SupervisorInterface::Register(Envelope {
                        msg: RegisterChild(spec),
                        req: request,
                    }) => {
                        request.reply(self.inner.add_spec(spec)).ok();
                    }
                    SupervisorInterface::Deregister(Envelope {
                        msg: DeregisterChild(pid),
                        req: request,
                    }) => {
                        request
                            .reply(self.remove_spec(&pid).map(|s| s.get_description()))
                            .ok();
                    }
                },
            },

            InnerNext::Source(ev) => match ev {
                SupervisorSourceEvent::Added(spec) => {
                    if let Err(e) = self.inner.add_spec(spec) {
                        tracing::warn!(%e, "Failed to add supervisee from source");
                    }
                }
                SupervisorSourceEvent::Removed(pid) => {
                    self.remove_spec(&pid);
                }
            },

            InnerNext::Supervisee(SuperviseeNext { pid, item }) => match item {
                SuperviseeItem::Started(Ok(())) => {}
                SuperviseeItem::Started(Err(error)) => {
                    tracing::warn!(%error, "Supervisee failed to start");
                    self.handle_exit(&pid, ExitReason::StartFailure)?;
                }

                SuperviseeItem::Exit(exit) => {
                    if let SuperviseeExit::JoinError(e) = &exit {
                        tracing::warn!(%e, "Supervisee exited with a join error");
                    }
                    self.handle_exit(&pid, ExitReason::Exit(exit))?;
                }

                SuperviseeItem::Initialized(Ok(())) => {
                    self.handle_initialized(&pid);
                }
                SuperviseeItem::Initialized(Err(status)) => {
                    tracing::warn!(%status, "Supervisee exited before finishing initialization");
                    self.handle_exit(&pid, ExitReason::InitExit(status))?;
                }
            },
        }

        ControlFlow::Continue(())
    }

    fn handle_exit(&mut self, pid: &Pid, reason: ExitReason) -> ControlFlow<()> {
        if self.exiting.swap_remove(pid) {
            return match self.exiting.is_empty() {
                true => ControlFlow::Break(()),
                false => ControlFlow::Continue(()),
            };
        }

        if self.restarting.swap_remove(pid) {
            if self.restarting.is_empty() {
                self.finish_cascade();
            }
            return ControlFlow::Continue(());
        }

        let Some(supervisee) = self.inner.supervisees.get_mut(&pid) else {
            tracing::warn!("Supervisee not found for pid: {}", pid);
            return ControlFlow::Continue(());
        };

        if !reason.requires_restart(supervisee) {
            self.remove_spec(&pid);
            return ControlFlow::Continue(());
        }

        if !supervisee.acquire_restart_permit() {
            return self.shutdown();
        }

        // One-for-all: this child alone decides *whether* the group restarts,
        // but once it does, every other live supervisee is stopped and
        // restarted alongside it (see `start_cascade` / `finish_cascade`).
        self.cascade_restart.insert(pid.clone());
        self.start_cascade(pid)
    }

    /// Stops every other currently-alive supervisee that isn't already part of
    /// the cascade, folding them in so they get restarted (or dropped, if
    /// configured with `RestartMode::Never`) once every member has exited.
    ///
    /// `trigger` is the pid whose exit started (or is rejoining) the cascade;
    /// it has already exited and does not need to be stopped itself.
    fn start_cascade(&mut self, trigger: &Pid) -> ControlFlow<()> {
        let siblings: Vec<Pid> = self
            .inner
            .supervisees
            .pids()
            .filter(|pid| {
                *pid != trigger
                    && !self.cascade_restart.contains(*pid)
                    && !self.cascade_drop.contains(*pid)
            })
            .cloned()
            .collect();

        for pid in siblings {
            let Some(supervisee) = self.inner.supervisees.get_mut(&pid) else {
                continue;
            };

            if supervisee.cfg().restart_mode == RestartMode::Never {
                self.cascade_drop.insert(pid.clone());
                if supervisee.stop().is_shutting_down() {
                    self.restarting.insert(pid);
                }
                continue;
            }

            if !supervisee.acquire_restart_permit() {
                return self.shutdown();
            }

            self.cascade_restart.insert(pid.clone());
            if supervisee.stop().is_shutting_down() {
                self.restarting.insert(pid);
            }
        }

        if self.restarting.is_empty() {
            self.finish_cascade();
        }

        ControlFlow::Continue(())
    }

    /// Called once every member of the current cascade has exited: drops the
    /// ones configured with `RestartMode::Never` and restarts the rest.
    fn finish_cascade(&mut self) {
        for pid in self.cascade_drop.drain(..).collect::<Vec<_>>() {
            self.remove_spec(&pid);
        }

        for pid in self.cascade_restart.drain(..) {
            let Some(supervisee) = self.inner.supervisees.get_mut(&pid) else {
                continue;
            };

            if let Err(e) = supervisee.start() {
                tracing::error!(%e, "Failed to restart supervisee");
            }
        }
    }

    fn handle_initialized(&mut self, pid: &Pid) {
        self.inner.handle_initialized(&mut self.initializing, pid)
    }

    fn shutdown(&mut self) -> ControlFlow<()> {
        self.inner.shutdown(&mut self.exiting)
    }

    fn remove_spec(&mut self, pid: &Pid) -> Option<Supervisee> {
        let supervisee = self
            .inner
            .remove_spec(&mut self.initializing, &mut self.exiting, pid);

        if supervisee.is_some() {
            if self.restarting.swap_remove(pid) && self.restarting.is_empty() {
                self.finish_cascade();
            }

            self.cascade_restart.swap_remove(pid);
            self.cascade_drop.swap_remove(pid);
        }

        supervisee
    }
}
