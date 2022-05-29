use crate::core::*;
use futures::{Future, FutureExt};
use log::{info, warn};
use std::{
    any::{Any, TypeId},
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

pub type ActorChild<A> = Child<<A as Actor>::Exit>;

#[derive(Debug)]
#[must_use = "If the child is dropped, the actor will be aborted."]
pub struct Child<T: Send + 'static> {
    handle: Option<JoinHandle<T>>,
    process_id: ProcessId,
    signal_sender: Option<Snd<ChildSignal>>,
    to_abort: bool,
    abort_timeout: Duration,
    exited: Arc<AtomicBool>,
}

impl<T: Send + 'static> Child<T> {
    pub(crate) fn new(
        handle: JoinHandle<T>,
        process_id: ProcessId,
        signal_sender: Snd<ChildSignal>,
        abort_timeout: Duration,
        exited: Arc<AtomicBool>,
    ) -> Self {
        Self {
            handle: Some(handle),
            process_id,
            signal_sender: Some(signal_sender),
            to_abort: true,
            abort_timeout,
            exited,
        }
    }

    pub fn soft_abort(&mut self) -> bool {
        match self.signal_sender.take() {
            Some(signal_sender) => {
                let _ = signal_sender.send(ChildSignal::SoftAbort);
                true
            }
            None => false,
        }
    }
}

impl<T: Send + 'static> Future for Child<T> {
    type Output = Result<T, ExitError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.handle
            .as_mut()
            .unwrap()
            .poll_unpin(cx)
            .map(|result| match result {
                Ok(exit) => Ok(exit),
                Err(e) => {
                    if e.is_cancelled() {
                        Err(ExitError::HardAbort)
                    } else {
                        Err(ExitError::Panic(e.into_panic()))
                    }
                }
            })
    }
}

impl<T: Send + 'static> Unpin for Child<T> {}

impl<T: Send + 'static> Drop for Child<T> {
    fn drop(&mut self) {
        // If we should not abort, or if the child has already exited, don't do anything
        let exited = self.exited.load(Ordering::Relaxed);
        if !self.to_abort || exited {
            // info!(
            //     "Child dropped, no abort. to_abort={}, exited={}",
            //     self.to_abort, exited
            // );
            return;
        }

        // If there is still a signal sender, send a soft abort message first
        if let Some(signal_sender) = self.signal_sender.take() {
            let _ = signal_sender.send(ChildSignal::SoftAbort);
            // info!("Child dropped, soft abort has been sent");
        }

        // Then spawn a task which will hard abort the process in 1000 ms
        let handle = self.handle.take().unwrap();
        let exited_arc = self.exited.clone();
        tokio::task::spawn(async move {
            let instant = tokio::time::Instant::now()
                .checked_add(Duration::from_millis(10_000))
                .unwrap();
            tokio::time::sleep_until(instant).await;
            if !exited_arc.load(Ordering::Relaxed) {
                warn!("Child dropped, hard aborting!");
                handle.abort()
            }
        });
    }
}

//------------------------------------------------------------------------------------------------
//  ExitError
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub enum ExitError {
    HardAbort,
    Panic(Box<dyn Any + Send>),
}

impl From<JoinError> for ExitError {
    fn from(e: JoinError) -> Self {
        if e.is_panic() {
            ExitError::Panic(e.into_panic())
        } else {
            ExitError::HardAbort
        }
    }
}
