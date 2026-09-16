use std::{collections::VecDeque, ops::ControlFlow};

use super::*;

pub(super) struct RestForOneSupervisor<'a> {
    inner: &'a mut SupervisorInner,
    initializing: IndexSet<Pid>,
    exiting: IndexSet<Pid>,
    cascade: Cascade,
}

/// A rest-for-one restart in progress: everything after the pid that
/// triggered it is torn down one at a time, in reverse start order, then
/// the trigger and survivors are started back up one at a time, in start
/// order, each one waiting for the previous to finish initializing before
/// the next is spawned.
enum Cascade {
    Idle,

    /// Stopping siblings after the trigger, one at a time. `current` is the
    /// one we're waiting to hear an exit from; `pending` holds the rest not
    /// yet told to stop, next-to-process at the back (i.e. reverse start
    /// order).
    Stopping {
        current: Pid,
        pending: Vec<Pid>,
        /// Accumulated in the order stopped (reverse start order); reversed
        /// into start order once every pid has stopped.
        to_restart: Vec<Pid>,
        /// Siblings configured with `RestartMode::Never`: stopped like
        /// everyone else, but dropped instead of restarted once they're dead.
        to_drop: Vec<Pid>,
    },

    /// Restarting the cascade's members one at a time, in start order.
    /// `current` is the one we're waiting to finish initializing.
    Restarting {
        current: Pid,
        pending: VecDeque<Pid>,
    },
}

impl Cascade {
    fn current(&self) -> Option<&Pid> {
        match self {
            Cascade::Idle => None,
            Cascade::Stopping { current, .. } | Cascade::Restarting { current, .. } => {
                Some(current)
            }
        }
    }

    /// Whether `pid` is already queued to be stopped by this cascade, just
    /// not reached yet (still alive, so it's able to exit on its own before
    /// its turn comes up).
    fn pending_contains(&self, pid: &Pid) -> bool {
        match self {
            Cascade::Stopping { pending, .. } => pending.contains(pid),
            Cascade::Idle | Cascade::Restarting { .. } => false,
        }
    }
}

impl<'a> RestForOneSupervisor<'a> {
    pub(super) fn new(inner: &'a mut SupervisorInner) -> Self {
        Self {
            inner,
            initializing: IndexSet::new(),
            exiting: IndexSet::new(),
            cascade: Cascade::Idle,
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
                        request,
                    }) => {
                        request.reply(self.inner.add_spec(spec)).ok();
                    }
                    SupervisorInterface::Deregister(Envelope {
                        msg: DeregisterChild(pid),
                        request,
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
                    self.handle_exit(&pid, ExitReason::Start)?;
                }
                SuperviseeItem::Initialized(Err(status)) => {
                    tracing::warn!(%status, "Supervisee exited before finishing initialization");
                    self.handle_exit(&pid, ExitReason::Start)?;
                }
                SuperviseeItem::Exit(exit) => {
                    if let SuperviseeExit::JoinError(e) = &exit {
                        tracing::warn!(%e, "Supervisee exited with a join error");
                    }
                    self.handle_exit(&pid, ExitReason::Exit(exit))?;
                }
                SuperviseeItem::Initialized(Ok(())) => {
                    self.handle_initialized(&pid);

                    // A restart step in our cascade just finished
                    // initializing; only now move on to the next one.
                    if self.cascade.current() == Some(&pid) {
                        self.advance_cascade()?;
                    }
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

        if self.cascade.current() == Some(pid) {
            return self.advance_cascade();
        }

        if self.cascade.pending_contains(pid) {
            // Already queued by the in-progress cascade (it just happened to
            // exit on its own before its turn came up); it'll be handled
            // when the chain reaches it, nothing to do here.
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

        match std::mem::replace(&mut self.cascade, Cascade::Idle) {
            Cascade::Idle => self.start_cascade(pid),

            Cascade::Stopping {
                current,
                pending,
                to_restart,
                to_drop,
            } => self.merge_into_stopping(pid.clone(), current, pending, to_restart, to_drop),

            Cascade::Restarting { .. } => {
                // A restart was already underway; this trigger means
                // everything after it must be torn down again, possibly
                // including things we just brought back up. Rather than try
                // to splice that into an active restart, cancel it and
                // start a clean cascade from this trigger: it recomputes
                // the full range from live state, so anything still
                // mid-restart gets correctly stopped again.
                self.start_cascade(pid)
            }
        }
    }

    /// Begins a rest-for-one cascade: everything started after `trigger`
    /// gets stopped one at a time, in reverse start order.
    fn start_cascade(&mut self, trigger: &Pid) -> ControlFlow<()> {
        let all: Vec<Pid> = self.inner.supervisees.pids().cloned().collect();
        let idx = all
            .iter()
            .position(|p| p == trigger)
            .expect("trigger is a known supervisee");
        let pending = all[idx + 1..].to_vec();

        self.advance_stop(pending, vec![trigger.clone()], Vec::new())
    }

    /// Folds a newly-crashed `trigger` into a cascade that's still stopping
    /// siblings, widening its scope to cover everything between `trigger`
    /// and whichever end of the current scope it falls outside of (below the
    /// oldest pending pid, or above `current`, e.g. a pid added after the
    /// cascade started). `current` itself is left untouched; whatever it
    /// finishes into will resume the (now wider) queue as usual.
    fn merge_into_stopping(
        &mut self,
        trigger: Pid,
        current: Pid,
        old_pending: Vec<Pid>,
        mut to_restart: Vec<Pid>,
        to_drop: Vec<Pid>,
    ) -> ControlFlow<()> {
        let all: Vec<Pid> = self.inner.supervisees.pids().cloned().collect();
        let pos_of = |target: &Pid| {
            all.iter()
                .position(|p| p == target)
                .expect("known supervisee")
        };

        let trigger_idx = pos_of(&trigger);
        let current_idx = pos_of(&current);
        let low = trigger_idx.min(current_idx);
        let high = trigger_idx.max(current_idx);

        // Already-settled pids (stopped and waiting to be dropped or
        // restarted) can fall inside [low, high] when `trigger` is above
        // `current` (e.g. a pid added after the cascade started); they don't
        // need stopping again.
        let already_settled: std::collections::HashSet<Pid> =
            to_restart.iter().chain(to_drop.iter()).cloned().collect();

        let mut pending: Vec<Pid> = all[low + 1..high]
            .iter()
            .filter(|p| !already_settled.contains(*p))
            .cloned()
            .chain(old_pending)
            .collect();
        pending.sort_by_key(|p| pos_of(p));
        pending.dedup();

        to_restart.push(trigger);

        self.cascade = Cascade::Stopping {
            current,
            pending,
            to_restart,
            to_drop,
        };
        ControlFlow::Continue(())
    }

    /// Resumes whichever phase of the cascade is currently waiting, now that
    /// its `current` member has finished (stopped, initialized, or been
    /// forced out via a manual deregistration).
    fn advance_cascade(&mut self) -> ControlFlow<()> {
        match std::mem::replace(&mut self.cascade, Cascade::Idle) {
            Cascade::Idle => ControlFlow::Continue(()),
            Cascade::Stopping {
                pending,
                to_restart,
                to_drop,
                ..
            } => self.advance_stop(pending, to_restart, to_drop),
            Cascade::Restarting { pending, .. } => self.advance_restart(pending),
        }
    }

    /// Stops the next pid in `pending` (from the back, i.e. reverse start
    /// order). Once `pending` is empty, drops whatever was `RestartMode::Never`
    /// and moves on to restarting the rest.
    fn advance_stop(
        &mut self,
        mut pending: Vec<Pid>,
        mut to_restart: Vec<Pid>,
        mut to_drop: Vec<Pid>,
    ) -> ControlFlow<()> {
        loop {
            let Some(next) = pending.pop() else {
                for pid in to_drop {
                    self.remove_spec(&pid);
                }
                to_restart.reverse();
                return self.advance_restart(to_restart.into());
            };

            let Some(supervisee) = self.inner.supervisees.get_mut(&next) else {
                continue; // already gone; nothing to wait for
            };

            if supervisee.cfg().restart_mode == RestartMode::Never {
                to_drop.push(next.clone());
                if supervisee.stop().is_shutting_down() {
                    self.cascade = Cascade::Stopping {
                        current: next,
                        pending,
                        to_restart,
                        to_drop,
                    };
                    return ControlFlow::Continue(());
                }
                continue;
            }

            if !supervisee.acquire_restart_permit() {
                return self.shutdown();
            }

            to_restart.push(next.clone());
            if supervisee.stop().is_shutting_down() {
                self.cascade = Cascade::Stopping {
                    current: next,
                    pending,
                    to_restart,
                    to_drop,
                };
                return ControlFlow::Continue(());
            }
        }
    }

    /// Starts the next pid in `pending` (from the front, i.e. start order).
    /// The cascade only moves on to the one after it once this one reports
    /// `SuperviseeItem::Initialized` (see `handle_next`).
    fn advance_restart(&mut self, mut pending: VecDeque<Pid>) -> ControlFlow<()> {
        loop {
            let Some(next) = pending.pop_front() else {
                self.cascade = Cascade::Idle;
                return ControlFlow::Continue(());
            };

            let Some(supervisee) = self.inner.supervisees.get_mut(&next) else {
                continue; // removed in the meantime; nothing to wait for
            };

            match supervisee.start() {
                Ok(true) => {
                    self.cascade = Cascade::Restarting {
                        current: next,
                        pending,
                    };
                    return ControlFlow::Continue(());
                }
                Ok(false) => continue, // already starting/alive; nothing new to wait for
                Err(e) => {
                    tracing::error!(%e, "Failed to restart supervisee");
                }
            }
        }
    }

    fn handle_initialized(&mut self, pid: &Pid) {
        shared::handle_initialized(self.inner, &mut self.initializing, pid)
    }

    fn shutdown(&mut self) -> ControlFlow<()> {
        shared::shutdown(self.inner, &mut self.exiting)
    }

    fn remove_spec(&mut self, pid: &Pid) -> Option<Supervisee> {
        let supervisee =
            shared::remove_spec(self.inner, &mut self.initializing, &mut self.exiting, pid);

        if supervisee.is_some() && self.cascade.current() == Some(pid) {
            // The pid our cascade was waiting on just got yanked out from
            // under it; treat that the same as it finishing on its own so
            // the rest of the chain still proceeds.
            let _ = self.advance_cascade();
        }

        supervisee
    }
}
