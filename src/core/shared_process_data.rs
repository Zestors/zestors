use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::*;

#[derive(Debug)]
pub(crate) struct SharedProcessData {
    process_id: ProcessId,
    has_exited: AtomicBool,
}

impl SharedProcessData {
    pub(crate) fn new(process_id: ProcessId) -> Self {
        Self {
            process_id,
            has_exited: AtomicBool::new(false),
        }
    }

    pub(crate) fn has_exited(&self) -> bool {
        self.has_exited.load(Ordering::Relaxed)
    }

    pub(crate) fn set_exited(&self) {
        self.has_exited.store(true, Ordering::Relaxed);
    }

    pub(crate) fn process_id(&self) -> ProcessId {
        self.process_id
    }
}
