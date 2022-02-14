use std::any::Any;

use futures::{Future, FutureExt};
use uuid::Uuid;

use crate::{
    abort::AbortSender,
    actor::{Actor, InternalExitReason},
    address::{Address, Addressable, RawAddress},
};

//--------------------------------------------------------------------------------------------------
//  Process
//--------------------------------------------------------------------------------------------------

/// A [Process] is a handle to an [Actor], there can only ever be one single [Process]
/// for a single [Actor]. [Process]es can be used to build supervision trees, since by default
/// if a process is dropped, the actor be aborted..
///
/// This abort is at first a soft_abort message, which the actor can handle gracefully.
/// If the timeout has passed ([Actor::ABORT_TIMER]), and the [Actor] has not yet exited, then it
/// will be forcefully aborted with hard_abort.
///
/// A Process can be awaited to return the [ProcessExit] value.
pub struct Process<A: Actor> {
    handle: Option<tokio::task::JoinHandle<InternalExitReason<A>>>,
    abort_sender: Option<AbortSender>,
    address: A::Address,
    is_attached: bool,
    registration: Option<Uuid>
}

//--------------------------------------------------------------------------------------------------
//  implement Process
//--------------------------------------------------------------------------------------------------

impl<A: Actor> Process<A> {
    pub(crate) fn new(
        handle: tokio::task::JoinHandle<InternalExitReason<A>>,
        address: A::Address,
        abort_sender: AbortSender,
        attached: bool,
    ) -> Self {
        Self {
            handle: Some(handle),
            abort_sender: Some(abort_sender),
            address,
            is_attached: attached,
            registration: None
        }
    }

    /// Whether this process is still alive.
    pub fn is_alive(&self) -> bool {
        self.address.raw_address().is_alive()
    }

    /// Whether this process has already been soft-aborted. Soft-aborting can only be done once.
    pub fn is_soft_aborted(&self) -> bool {
        self.abort_sender.is_none()
    }

    /// Whether this process is attached.
    pub fn is_attached(&self) -> bool {
        self.is_attached
    }

    /// detach this process, it will not be aborted if this [Process] is dropped. If called on
    /// a process which has already been detached, nothing happens.
    pub fn detach(&mut self) {
        self.is_attached = false;
    }

    /// Attach this process, it will now be aborted if this [Process] is dropped. The actor
    /// will first receive a soft_abort, with a hard_abort after the abort_timer set by the [Actor].
    ///
    /// If called on a process which is still attached, nothing happens.
    pub fn re_attach(&mut self) {
        self.is_attached = true;
    }

    /// Hard abort this process at this moment. It will not receive a soft_abort message
    /// before.
    ///
    /// If called on a process which has already aborted, nothing happens.
    ///
    /// If called on a process which is in the middle of a soft-abort, then it will be hard-aborted
    /// instead.
    pub fn hard_abort(&mut self) {
        // The handle should only be taken out by the Drop implementation, so there should
        // always be a handle here.
        self.handle.as_ref().unwrap().abort();
    }

    /// Soft abort this process a this moment. It will receive a soft-abort message,
    /// and can exit gracefully. This will not hard-abort after a delay, that will only happen
    /// by manually calling `Process::hard_abort()`.
    ///
    /// If this [Process] is dropped before the [Actor] had time to exit gracefully, the standard
    /// abort procedure will start anyway, with an abort_timer set to the [Actor] default.
    ///
    /// Soft-abortion can only be done once. If this process has already been soft-aborted, then
    /// nothing happens and this function returns false. If the process has not been soft-aborted
    /// yet, then this function returns true.
    ///
    /// If the process has already exited before this function is called, then nothing happens.
    pub fn soft_abort(&mut self) -> bool {
        if let Some(abort_sender) = self.abort_sender.take() {
            abort_sender.send_soft_abort();
            true
        } else {
            false
        }
    }
}

//--------------------------------------------------------------------------------------------------
//  implement traits for Process
//--------------------------------------------------------------------------------------------------

impl<A: Actor> Unpin for Process<A> {}

impl<A: Actor> Future for Process<A> {
    type Output = ProcessExit<A>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.handle
            .as_mut()
            .unwrap() // Handle should only be taken out when dropped
            .poll_unpin(cx)
            .map(|res| match res {
                Ok(some) => match some {
                    InternalExitReason::InitFailed => ProcessExit::InitFailed,
                    InternalExitReason::Handled(returns) => ProcessExit::Handled(returns),
                },
                Err(e) => match e.is_panic() {
                    true => ProcessExit::Panic(e.into_panic()),
                    false => ProcessExit::HardAbort,
                },
            })
    }
}

impl<A: Actor> Drop for Process<A> {
    fn drop(&mut self) {
        // If the process is attached and alive, abort it.
        if self.is_attached && self.is_alive() {
            // This is the only place where the handle is taken out.
            let handle = self.handle.take().unwrap();

            // The process should receive soft_abort first
            if let Some(abort_sender) = self.abort_sender.take() {
                tokio::task::spawn(async move {
                    abort_sender.send_soft_abort();
                    tokio::time::sleep(A::ABORT_TIMER).await;
                    handle.abort();
                });
            // The process should only receive a hard_abort
            } else {
                tokio::task::spawn(async move {
                    tokio::time::sleep(A::ABORT_TIMER).await;
                    handle.abort();
                });
            }
        }
    }
}

impl<A: Actor> RawAddress for Process<A> {
    type Actor = A;
    
    fn raw_address(&self) -> &Address<A> {
        &self.address.raw_address()
    }
}

impl<A: Actor> Addressable<A> for Process<A> {}

//--------------------------------------------------------------------------------------------------
//  ProcessExit
//--------------------------------------------------------------------------------------------------

/// The reason why a process exited. Returned when a [Process] is awaited.
#[derive(Debug)]
pub enum ProcessExit<A: Actor> {
    /// The actor has handled it's exit in a proper manner
    Handled(A::ExitWith),
    /// The initialisation of this actor has failed
    InitFailed,
    /// The actor has panicked. Inside is the panic information.
    Panic(Box<dyn Any + Send>),
    /// The process has been hard-aborted
    HardAbort,
}
