use std::ops::ControlFlow;

use super::*;

pub(super) struct OneForOneSupervisor<'a> {
    inner: &'a mut SupervisorInner,
    initializing: IndexSet<Pid>,
    exiting: IndexSet<Pid>,
}

impl<'a> OneForOneSupervisor<'a> {
    pub(super) fn new(inner: &'a mut SupervisorInner) -> Self {
        Self {
            inner,
            initializing: IndexSet::new(),
            exiting: IndexSet::new(),
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

                SuperviseeItem::Initialized(Ok(())) => {
                    self.handle_initialized(&pid);
                }
                SuperviseeItem::Initialized(Err(status)) => {
                    tracing::warn!(%status, "Supervisee exited before finishing initialization");
                    self.handle_exit(&pid, ExitReason::InitExit(status))?;
                }

                SuperviseeItem::Exit(exit) => {
                    if let SuperviseeExit::JoinError(e) = &exit {
                        tracing::warn!(%e, "Supervisee exited with a join error");
                    }
                    self.handle_exit(&pid, ExitReason::Exit(exit))?;
                }
            },
        }

        ControlFlow::Continue(())
    }

    fn handle_exit(&mut self, pid: &Pid, reason: ExitReason) -> ControlFlow<()> {
        if self.exiting.swap_remove(pid) {
            return if self.exiting.is_empty() {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            };
        }

        let Some(supervisee) = self.inner.supervisees.get_mut(&pid) else {
            tracing::warn!("Supervisee not found for pid: {}", pid);
            return ControlFlow::Continue(());
        };

        if !reason.requires_restart(supervisee) {
            self.remove_spec(&pid);
            return ControlFlow::Continue(());
        }

        match self.inner.restarter.acquire_permit() && supervisee.acquire_restart_permit() {
            true => {
                if let Err(e) = supervisee.start() {
                    tracing::error!(%e, "Failed to restart supervisee");
                }
            }
            false => {
                self.shutdown()?;
            }
        }

        ControlFlow::Continue(())
    }

    fn handle_initialized(&mut self, pid: &Pid) {
        self.inner.handle_initialized(&mut self.initializing, pid)
    }

    fn shutdown(&mut self) -> ControlFlow<()> {
        self.inner.shutdown(&mut self.exiting)
    }

    fn remove_spec(&mut self, pid: &Pid) -> Option<Supervisee> {
        self.inner
            .remove_spec(&mut self.initializing, &mut self.exiting, pid)
    }
}
