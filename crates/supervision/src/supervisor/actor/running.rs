use super::*;

pub(super) struct RunningSupervisor {
    supervisor: BaseSupervisor,
}

impl RunningSupervisor {
    pub fn new(supervisor: BaseSupervisor) -> Self {
        Self { supervisor }
    }

    pub async fn run(mut self) -> Result<(), ExitingSupervisor> {
        loop {
            match self.next().await {
                Next::InboxEvent(msg) => match msg {
                    InboxEvent::Signal(signal) => match signal {
                        Signal::Resume | Signal::Suspend => (),
                        Signal::Shutdown => {
                            tracing::info!("Supervisor received shutdown signal");
                            self.supervisor.stop_supervisees();
                            break Err(ExitingSupervisor::new(self.supervisor));
                        }
                    },
                    InboxEvent::Message(msg) => {
                        self.supervisor.handle_msg(msg);
                    }
                },

                Next::SuperviseeItem(pid, item) => {
                    match self.handle_supervisee_item(pid, item).await {
                        Ok(this) => self = this,
                        Err(e) => break Err(e),
                    }
                }
            }
        }
    }

    async fn handle_supervisee_item(
        mut self,
        pid: Pid,
        item: SuperviseeItem,
    ) -> Result<Self, ExitingSupervisor> {
        match item {
            SuperviseeItem::Started => {}

            SuperviseeItem::StartError(e) => {
                tracing::error!("Supervisee {pid} failed to start: {e:?}");
                self.supervisor.stop_supervisees();
                return Err(ExitingSupervisor::new(self.supervisor));
            }

            SuperviseeItem::Exit(exit) => {
                tracing::info!("Supervisee {pid} exited: {exit:?}");
                let supervisee = self.supervisor.get(&pid).expect("Should exist");

                // If it shouldn't be restarted, disable it forever and continue
                if !exit.should_restart(supervisee.cfg().restart_mode) {
                    self.supervisor.supervisees.remove(&pid);
                    return Ok(self);
                }

                todo!()
            }

            SuperviseeItem::Initialized => {}
        };

        Ok(self)
    }

    async fn next(&mut self) -> Next {
        // Supervisee-items must be handled over inbox events, since there may be a
        // backlog of supervisee-items when the supervisor is initially set to the
        // running state.
        // In theory, this could be reversed once the backlog is cleared.
        // Don't change the order without thinking this through properly, and clearing
        // the backlog before transitioning into the running state.
        tokio::select! {
            biased;

            Some(next) = self.supervisor.supervisees.next() => {
                Next::SuperviseeItem(next.pid, next.item)
            }

            Some(msg) = self.supervisor.inbox.recv_event() => {
                Next::InboxEvent(msg)
            }

            _ = future::pending::<()>() => { unreachable!() }
        }
    }
}

enum Next {
    InboxEvent(InboxEvent<SupervisorInterface>),
    SuperviseeItem(Pid, SuperviseeItem),
}
