use std::ops::ControlFlow;

use super::*;

/// Shared by every restart strategy: pulls this supervisor's own inbox into
/// `Stopping` and stops every currently-alive supervisee, returning `Break`
/// once there's nothing left to wait for.
pub(super) fn shutdown(
    inner: &mut SupervisorInner,
    exiting: &mut IndexSet<Pid>,
) -> ControlFlow<()> {
    inner.inbox.register_stopping();

    exiting.extend(inner.supervisees.stop_all());

    if exiting.is_empty() {
        ControlFlow::Break(())
    } else {
        ControlFlow::Continue(())
    }
}

pub(super) fn handle_initialized(
    inner: &mut SupervisorInner,
    initializing: &mut IndexSet<Pid>,
    pid: &Pid,
) {
    if initializing.swap_remove(pid) && inner.is_initializing() && initializing.is_empty() {
        inner.inbox.register_initialized();
    }
}

/// The part of `remove_spec` common to every strategy; strategies with extra
/// per-pid bookkeeping (e.g. an in-progress restart cascade) clean that up
/// themselves around this call.
pub(super) fn remove_spec(
    inner: &mut SupervisorInner,
    initializing: &mut IndexSet<Pid>,
    exiting: &mut IndexSet<Pid>,
    pid: &Pid,
) -> Option<Supervisee> {
    let supervisee = inner.supervisees.remove(pid);

    if supervisee.is_some() {
        initializing.swap_remove(pid);
        exiting.swap_remove(pid);
    }

    supervisee
}
