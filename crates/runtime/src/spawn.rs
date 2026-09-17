use super::*;
use futures::FutureExt as _;
use std::{convert::Infallible, panic::AssertUnwindSafe};
use tokio::task_local;
use tracing::Instrument as _;

task_local! {
    static PID_INFO: PidInfo;
}

#[derive(Clone, Debug)]
struct PidInfo {
    this: Pid,
    parent: Option<Pid>,
}

/// Same as [`spawn_with`], but spawns a process that cannot accept messages.
pub fn spawn_task_with<E, F>(
    pid: Pid,
    f: impl FnOnce(TaskBox) -> F,
) -> Result<Child<E, Infallible>, DuplicatePidError>
where
    E: Send + 'static,
    F: Future<Output = Result<E, rootcause::Report>> + Send + 'static,
{
    Ok(StrongAddress::create(pid)?
        .spawn_task(f)
        .expect("Address was just created. Must be valid"))
}

/// Same as [`spawn`], but spawns a process that cannot accept messages.
pub fn spawn_task<E, F>(f: impl FnOnce(TaskBox) -> F) -> Child<E, Infallible>
where
    E: Send + 'static,
    F: Future<Output = Result<E, rootcause::Report>> + Send + 'static,
{
    spawn_task_with(Pid::rand(), f).expect("Pid is unique")
}

/// Spawns a process on a new [`StrongAddress`] with the given [`Pid`], and
/// registers it in the [`Registry`].
///
/// Can fail if the pid is already registered.
pub fn spawn_with<T, E, F>(
    pid: Pid,
    f: impl FnOnce(Inbox<T>) -> F,
) -> Result<Child<E, T>, DuplicatePidError>
where
    T: Interface,
    E: Send + 'static,
    F: Future<Output = Result<E, rootcause::Report>> + Send + 'static,
{
    Ok(StrongAddress::create(pid)?
        .spawn(f)
        .expect("Address was just created. Must be valid"))
}

/// Spawns a process on a new [`StrongAddress`] with a random [`Pid`], and
/// registers it in the [`Registry`].
pub fn spawn<T, E, F>(f: impl FnOnce(Inbox<T>) -> F) -> Child<E, T>
where
    T: Interface,
    E: Send + 'static,
    F: Future<Output = Result<E, rootcause::Report>> + Send + 'static,
{
    spawn_with(Pid::rand(), f).expect("Pid is unique")
}

impl StrongAddress<Infallible> {
    pub fn spawn_task<E, F>(
        self,
        f: impl FnOnce(TaskBox) -> F,
    ) -> Result<Child<E, Infallible>, ConcurrentInboxError>
    where
        E: Send + 'static,
        F: Future<Output = Result<E, rootcause::Report>> + Send + 'static,
    {
        self.spawn(|inbox| f(inbox.into_task_box()))
    }
}

impl<T: Context> StrongAddress<T> {
    pub fn spawn<R, F>(
        self,
        spawn_fn: impl FnOnce(Inbox<T>) -> F,
    ) -> Result<Child<R, T>, ConcurrentInboxError>
    where
        T: Interface,
        R: Send + 'static,
        F: Future<Output = Result<R, Report>> + Send + 'static,
    {
        let tokio_handle = tokio::task::spawn({
            let span = tracing::debug_span!("process", pid = %self.pid());
            let inbox = Inbox::try_new(self.clone())?;
            let address = inbox.address().clone();
            let mut bomb = AbortBomb::new(address);
            bomb.address
                .register_spawned()
                .expect("Transition must succeed, because inbox was just created");
            let spawn_future = AssertUnwindSafe(spawn_fn(inbox)).catch_unwind();

            PID_INFO
                .scope(
                    PidInfo {
                        this: self.pid().clone(),
                        parent: Pid::current(),
                    },
                    async move {
                        let spawn_result = spawn_future.await;

                        let mapped_result = match spawn_result {
                            Ok(result) => {
                                match &result {
                                    Ok(_) => bomb.address.register_exited(Ok(())),
                                    Err(_) => bomb
                                        .address
                                        .register_exited(Err(ExitError::UnhandledError)),
                                };

                                result
                            }

                            Err(boxed) => {
                                bomb.address.register_exited(Err(ExitError::Panicked));
                                std::panic::resume_unwind(boxed);
                            }
                        };

                        bomb.defuse();

                        mapped_result
                    },
                )
                .instrument(span)
        });

        Ok(Child::new(tokio_handle, self))
    }
}

struct AbortBomb<T: Context> {
    address: Address<T>,
    armed: bool,
}

impl<T: Context> AbortBomb<T> {
    fn new(address: Address<T>) -> Self {
        Self {
            address,
            armed: true,
        }
    }

    fn defuse(&mut self) {
        self.armed = false;
    }
}

impl<T: Context> Drop for AbortBomb<T> {
    fn drop(&mut self) {
        if self.armed {
            tracing::debug!("AbortBomb triggered");

            if !self.address.status().is_dead() {
                self.address.register_exited(Err(ExitError::Aborted));
            }
        }
    }
}

pub(crate) fn current_pid() -> Option<Pid> {
    PID_INFO.try_with(|info| info.this.clone()).ok()
}

pub(crate) fn parent_pid() -> Option<Pid> {
    PID_INFO.try_with(|info| info.parent.clone()).ok().flatten()
}
