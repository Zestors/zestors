use std::ops::ControlFlow;

use super::*;

pub(super) struct OneForOneSupervisor<'a> {
    inner: &'a mut SupervisorInner,
    initializing: IndexSet<Name>,
    exiting: IndexSet<Name>,
}

impl<'a> OneForOneSupervisor<'a> {
    pub(super) fn new(inner: &'a mut SupervisorInner) -> Self {
        Self {
            inner,
            initializing: IndexSet::new(),
            exiting: IndexSet::new(),
        }
    }
}

impl<'a> Strategy for OneForOneSupervisor<'a> {
    fn parts(&mut self) -> (&mut SupervisorInner, &mut IndexSet<Name>) {
        (self.inner, &mut self.initializing)
    }

    fn handle_exit(&mut self, name: &Name, reason: ExitReason) -> ControlFlow<()> {
        if self.exiting.swap_remove(name) {
            return if self.exiting.is_empty() {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            };
        }

        let Some(supervisee) = self.inner.supervisees.get_mut(name) else {
            tracing::warn!("Supervisee not found for name: {}", name);
            return ControlFlow::Continue(());
        };

        if !reason.requires_restart(supervisee) {
            self.remove_spec(name);
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

    fn shutdown(&mut self) -> ControlFlow<()> {
        self.inner.shutdown(&mut self.exiting)
    }

    fn remove_spec(&mut self, name: &Name) -> Option<Supervisee> {
        self.inner
            .remove_spec(&mut self.initializing, &mut self.exiting, name)
    }
}
