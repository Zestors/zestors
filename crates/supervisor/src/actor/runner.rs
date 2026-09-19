use crate::messages::{DeregisterChild, RegisterChild};

use super::*;

/// The parts of a supervision strategy's event loop that don't vary by
/// strategy: [`Strategy::run`] and [`Strategy::handle_next`]'s inbox/source
/// dispatch and [`Strategy::handle_initialized`] are provided here as default
/// methods. Each strategy (`OneForOneSupervisor`, `OneForAllSupervisor`,
/// `RestForOneSupervisor`) only implements the parts that actually differ:
/// [`Strategy::handle_exit`], [`Strategy::shutdown`], and
/// [`Strategy::remove_spec`], plus [`Strategy::after_initialized`] for the one
/// strategy (rest-for-one) that needs to react to a supervisee finishing
/// initialization beyond the shared bookkeeping.
pub(super) trait Strategy {
    /// Splits `self` into its (always-present) shared state: the supervisor's
    /// shared inner state, and the set of supervisees still being waited on
    /// to finish initializing.
    fn parts(&mut self) -> (&mut SupervisorInner, &mut IndexSet<Name>);

    /// Handles a supervisee exiting (or failing to start), deciding whether
    /// it (or anything else) should be restarted, dropped, or should trigger
    /// a shutdown.
    fn handle_exit(&mut self, name: &Name, reason: ExitReason) -> ControlFlow<()>;

    /// Tears down this strategy's supervisees in whatever order/grouping the
    /// strategy requires. Idempotent: a shutdown already in progress is left
    /// alone. Returns `Break` once there's nothing left to wait for.
    fn shutdown(&mut self) -> ControlFlow<()>;

    /// Removes `name`'s spec, cleaning up any strategy-specific bookkeeping
    /// for it along the way (e.g. an in-progress restart cascade).
    fn remove_spec(&mut self, name: &Name) -> Option<Supervisee>;

    /// Called right after [`Strategy::handle_initialized`], for strategies
    /// that need to react to a supervisee finishing initialization beyond
    /// the shared bookkeeping. The default does nothing.
    fn after_initialized(&mut self, _name: &Name) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    /// Starts every supervisee and runs the event loop to completion.
    async fn run(&mut self) -> Result<(), Report> {
        let (inner, initializing) = self.parts();
        let start_result = inner.supervisees.start_all();
        *initializing = inner.supervisees.names().cloned().collect();

        // If some supervisees failed to start, tear down whichever ones did start
        // before bailing out, instead of leaving them running unsupervised.
        let should_run = match &start_result {
            Ok(()) => true,
            Err(_) => self.shutdown().is_continue(),
        };

        if should_run {
            while let Some(next) = self.parts().0.next().await {
                if let ControlFlow::Break(_) = self.handle_next(next) {
                    break;
                }
            }
        }

        start_result.map_err(|(name, e)| report!(e).attach(name).into())
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
                            .reply(self.parts().0.supervisees.child_descriptions())
                            .ok();
                    }
                    SupervisorInterface::Health(envelope) => {
                        envelope.reply(self.parts().0.health()).ok();
                    }
                    SupervisorInterface::Register(Envelope {
                        msg: RegisterChild(spec),
                        req: request,
                    }) => {
                        request.reply(self.parts().0.add_spec(spec)).ok();
                    }
                    SupervisorInterface::Deregister(Envelope {
                        msg: DeregisterChild(name),
                        req: request,
                    }) => {
                        request
                            .reply(self.remove_spec(&name).map(|s| s.get_description()))
                            .ok();
                    }
                },
            },

            InnerNext::Source(ev) => match ev {
                SupervisorSourceEvent::Added(spec) => {
                    if let Err(e) = self.parts().0.add_spec(spec) {
                        tracing::warn!(%e, "Failed to add supervisee from source");
                    }
                }
                SupervisorSourceEvent::Removed(name) => {
                    self.remove_spec(&name);
                }
            },

            InnerNext::Supervisee(SuperviseeNext { name, item }) => match item {
                SuperviseeItem::Started(Ok(())) => {}
                SuperviseeItem::Started(Err(error)) => {
                    tracing::warn!(%error, "Supervisee failed to start");
                    self.handle_exit(&name, ExitReason::StartFailure)?;
                }

                SuperviseeItem::Initialized(Ok(())) => {
                    self.handle_initialized(&name);
                    self.after_initialized(&name)?;
                }
                SuperviseeItem::Initialized(Err(status)) => {
                    tracing::warn!(%status, "Supervisee exited before finishing initialization");
                    self.handle_exit(&name, ExitReason::InitExit(status))?;
                }

                SuperviseeItem::Exit(exit) => {
                    if let SuperviseeExit::JoinError(e) = &exit {
                        tracing::warn!(%e, "Supervisee exited with a join error");
                    }
                    self.handle_exit(&name, ExitReason::Exit(exit))?;
                }
            },
        }

        ControlFlow::Continue(())
    }

    fn handle_initialized(&mut self, name: &Name) {
        let (inner, initializing) = self.parts();
        inner.handle_initialized(initializing, name);
    }
}
