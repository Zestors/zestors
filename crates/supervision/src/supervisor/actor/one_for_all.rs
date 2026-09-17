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
}

impl<'a> Strategy for OneForAllSupervisor<'a> {
    fn parts(&mut self) -> (&mut SupervisorInner, &mut IndexSet<Pid>) {
        (self.inner, &mut self.initializing)
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

        let Some(supervisee) = self.inner.supervisees.get_mut(pid) else {
            tracing::warn!("Supervisee not found for pid: {}", pid);
            return ControlFlow::Continue(());
        };

        if !reason.requires_restart(supervisee) {
            self.remove_spec(pid);
            return ControlFlow::Continue(());
        }

        if !self.inner.restarter.acquire_permit() || !supervisee.acquire_restart_permit() {
            return self.shutdown();
        }

        // One-for-all: this child alone decides *whether* the group restarts,
        // but once it does, every other live supervisee is stopped and
        // restarted alongside it (see `start_cascade` / `finish_cascade`).
        self.cascade_restart.insert(pid.clone());
        self.start_cascade(pid)
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
