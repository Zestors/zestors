use crate::core::*;
use futures::{Future, FutureExt};
use log::{info, warn};
use std::{
    any::{Any, TypeId},
    marker::PhantomData,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::task::{JoinError, JoinHandle};



//------------------------------------------------------------------------------------------------
//  Child
//------------------------------------------------------------------------------------------------

/// A child represents a unique handle to a child-process, used to build supervision trees.
/// By default, the process is attached to this process, and will be aborted when the `Child` is
/// dropped.
/// 
/// The process will be aborted by first sending a soft-abort message that can be gracefully
/// handled by the actor. If the actor has not exited before the abort-timeout has passed,
/// the process will be hard-aborted, by forcefully interrupting the process at a `.await` point.
/// 
/// This ensures that, if the processes from a tree, when the root-process is killed, all children
/// will be able to exit properly.
#[derive(Debug)]
#[must_use = "If the child is dropped, the actor will be aborted."]
pub struct Child<A: Actor> {
    handle: Option<JoinHandle<A::Exit>>,
    signal_sender: Option<Snd<ChildMsg>>,
    to_abort: bool,
    abort_timeout: Option<Duration>,
    shared: Arc<SharedProcessData>
}

impl<A: Actor> Child<A> {
    pub(crate) fn new(
        handle: JoinHandle<A::Exit>,
        signal_sender: Snd<ChildMsg>,
        abort_timeout: Option<Duration>,
        shared: Arc<SharedProcessData>
    ) -> Self {
        Self {
            handle: Some(handle),
            signal_sender: Some(signal_sender),
            to_abort: true,
            abort_timeout,
            shared
        }
    }

    /// Detatches the process from the supervision tree.
    ///
    /// The process will not be aborted when this child is dropped.
    pub fn detach(&mut self) {
        self.to_abort = false;
    }

    /// Re-attaches the process to the supervision tree.
    ///
    /// The process will be aborted when this child is dropped.
    pub fn re_attach(&mut self) {
        self.to_abort = true;
    }

    /// Returns whether the process has already exited.
    pub fn has_exited(&self) -> bool {
        self.shared.has_exited()
    }

    /// Get the process_id of this actor.
    pub fn process_id(&self) -> ProcessId {
        self.shared.process_id()
    }

    /// Sets the amount of time this child has to exit before a hard-abort will
    /// be sent.
    pub fn set_abort_timeout(&mut self, timeout: Duration) {
        self.abort_timeout = Some(timeout)
    }

    /// Disables the abort timeout.
    ///
    /// When this `Child` is dropped, the process will still receive a soft-abort,
    /// but will never be hard-aborted.
    pub fn disable_abort_timeout(&mut self) {
        self.abort_timeout = None
    }

    /// Send a soft-abort message to this actor. This can only be done once.
    ///
    /// Returns true if the soft-abort was sent, false otherwise.
    pub fn soft_abort(&mut self) -> bool {
        match self.signal_sender.take() {
            Some(signal_sender) => {
                let _ = signal_sender.send(ChildMsg::SoftAbort);
                true
            }
            None => false,
        }
    }
}

impl<A: Actor> Future for Child<A> {
    type Output = Result<A::Exit, ExitError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.handle
            .as_mut()
            .unwrap()
            .poll_unpin(cx)
            .map_err(|e| e.into())
    }
}

impl<A: Actor> Unpin for Child<A> {}

impl<A: Actor> Drop for Child<A> {
    fn drop(&mut self) {
        // If we should not abort, or if the child has already exited, don't do anything
        let exited = self.shared.has_exited();
        if !self.to_abort || exited {
            return;
        }

        // If there is still a signal sender, send a soft abort message first
        if let Some(signal_sender) = self.signal_sender.take() {
            let _ = signal_sender.send(ChildMsg::SoftAbort);
        }

        if let Some(timeout) = self.abort_timeout.take() {
            // Then spawn a task which will hard abort the process after the timeout
            let handle = self.handle.take().unwrap();
            let shared_arc = self.shared.clone();
            tokio::task::spawn(async move {
                let instant = tokio::time::Instant::now().checked_add(timeout).unwrap();
                tokio::time::sleep_until(instant).await;
                if !shared_arc.has_exited() {
                    warn!("Child dropped, hard aborting!");
                    handle.abort()
                }
            });
        }
    }
}

//------------------------------------------------------------------------------------------------
//  ExitError
//------------------------------------------------------------------------------------------------

/// An error returned for when an actor exit was not properply handled. This can be either because:
/// - The actor has been hard-aborted.
/// - The actor has panicked.
#[derive(Debug, ThisError)]
#[error("Unhandled actor exit: {}")]
pub enum ExitError {
    #[error("Actor has been hard-aborted.")]
    HardAbort,
    #[error("Actor has panicked.")]
    Panic(Box<dyn Any + Send>),
}

impl From<JoinError> for ExitError {
    fn from(e: JoinError) -> Self {
        match e.try_into_panic() {
            Ok(e) => ExitError::Panic(e),
            Err(_) => ExitError::HardAbort,
        }
    }
}
