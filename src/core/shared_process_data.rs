use std::sync::atomic::{AtomicBool, Ordering};

use futures::{Future, FutureExt};
use tokio::sync::Notify;

use crate::core::*;

/// Any data that should be accessable by the Child, Addr and State.
#[derive(Debug)]
pub(crate) struct SharedProcessData {
    // The immutable ProcessId.
    process_id: ProcessId,
    // Whether the process has exited.
    has_exited: AtomicBool,
    // A notifier to signal when the process is exiting.
    notify: Notify,
}

impl SharedProcessData {
    pub(crate) fn new(process_id: ProcessId) -> Self {
        Self {
            process_id,
            has_exited: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    pub(crate) async fn await_exit(&self) {
        // If the process has exited, we can return.
        if self.has_exited() {
            return;
        }
        // Otherwise, get a notifier
        let notified = self.notify.notified();
        // If the process exited in the meantime, return.
        if self.has_exited() {
            return;
        }
        // Otherwise, await the notifier
        notified.await;
    }

    pub(crate) fn has_exited(&self) -> bool {
        self.has_exited.load(Ordering::SeqCst)
    }

    pub(crate) fn exit(&self) {
        self.has_exited.store(true, Ordering::SeqCst);
        self.notify.notify_waiters()
    }

    pub(crate) fn process_id(&self) -> ProcessId {
        self.process_id
    }
}
