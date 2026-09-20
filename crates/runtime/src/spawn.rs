use super::*;
use futures::FutureExt as _;
use std::{convert::Infallible, panic::AssertUnwindSafe};
use tokio::task_local;
use tracing::Instrument as _;

task_local! {
    static NAME_INFO: NameInfo;
}

#[derive(Clone, Debug)]
struct NameInfo {
    this: Name,
    #[expect(unused)]
    parent: Option<Name>,
}

/// Same as [`spawn`], but spawns a process that cannot accept messages: its
/// handler gets a [`TaskBox`] instead of an [`Inbox`], through which it can
/// still receive [`Signal`]s (e.g. [`Signal::Shutdown`]) but no messages.
///
/// # Example
///
/// ```
/// # use zestors::runtime::prelude::*;
/// # use zestors::runtime::{TaskBox, spawn_task};
///
/// # #[tokio::main]
/// # async fn main() {
/// let child = spawn_task(Name::new("background-job"), |mut task_box: TaskBox| async move {
///     task_box.wait_shutdown().await;
///     Ok(())
/// })
/// .unwrap();
///
/// child.signal_shutdown();
/// child.await.unwrap();
/// # }
/// ```
pub fn spawn_task<E, F>(
    name: Name,
    f: impl FnOnce(TaskBox) -> F,
) -> Result<Child<E, Infallible>, DuplicateNameError>
where
    E: Send + 'static,
    F: Future<Output = Result<E, rootcause::Report>> + Send + 'static,
{
    Ok(StrongAddress::create(name)?
        .spawn_task(f)
        .expect("Address was just created. Must be valid"))
}

/// Same as [`spawn`], but spawns a process that cannot accept messages.
pub fn spawn_task_rand<E, F>(f: impl FnOnce(TaskBox) -> F) -> Child<E, Infallible>
where
    E: Send + 'static,
    F: Future<Output = Result<E, rootcause::Report>> + Send + 'static,
{
    spawn_task(Name::rand(), f).expect("Name is unique")
}

/// Spawns a process on a new [`StrongAddress`] with the given [`Name`], and
/// registers it in the [`Registry`].
///
/// Can fail if the name is already registered.
///
/// # Example
///
/// ```
/// # use zestors::runtime::prelude::*;
/// # use zestors::runtime::spawn;
/// # #[tokio::main]
/// # async fn main() {
/// let name = Name::new("my-actor");
/// let child = spawn(name.clone(), |mut inbox: Inbox<()>| async move {
///     while inbox.recv().await.is_some() {}
///     Ok(())
/// })
/// .unwrap();
///
/// // A second actor can't reuse the same name while this one is alive.
/// assert!(spawn(name, |_: Inbox<()>| async { Ok(()) }).is_err());
///
/// child.signal_shutdown();
/// # }
/// ```
pub fn spawn<T, E, F>(
    name: impl Into<Name>,
    f: impl FnOnce(Inbox<T>) -> F,
) -> Result<Child<E, T>, DuplicateNameError>
where
    T: Interface,
    E: Send + 'static,
    F: Future<Output = Result<E, rootcause::Report>> + Send + 'static,
{
    Ok(StrongAddress::create(name.into())?
        .spawn(f)
        .expect("Address was just created. Must be valid"))
}

/// Spawns a process on a new [`StrongAddress`] with a random [`Name`], and
/// registers it in the [`Registry`].
pub fn spawn_rand<T, E, F>(f: impl FnOnce(Inbox<T>) -> F) -> Child<E, T>
where
    T: Interface,
    E: Send + 'static,
    F: Future<Output = Result<E, rootcause::Report>> + Send + 'static,
{
    spawn(Name::rand(), f).expect("Name is unique")
}

impl StrongAddress<Infallible> {
    /// Same as [`StrongAddress::spawn`], but for a signal-only process (see
    /// [`spawn_task`]).
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
    /// Spawns a process on this channel, keeping its [`Name`] and registry
    /// entry. Fails with [`ConcurrentInboxError`] if a process is already
    /// running on it - so once one exits, [`StrongAddress`] is what lets you
    /// spawn another one *on the same channel*, rather than creating a new
    /// one from scratch with [`spawn`].
    ///
    /// # Example
    ///
    /// ```
    /// # use zestors::runtime::prelude::*;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let strong: StrongAddress<()> = StrongAddress::create(Name::rand()).unwrap();
    ///
    /// let first = strong.clone().spawn(|mut inbox: Inbox<()>| async move {
    ///     while inbox.recv().await.is_some() {}
    ///     Ok(())
    /// })
    /// .unwrap();
    /// first.signal_shutdown();
    /// first.monitor_exit().await.unwrap();
    ///
    /// // The first process is gone, but the name and registry entry live on,
    /// // so a second process can now be spawned on the very same channel.
    /// let second = strong.spawn(|mut inbox: Inbox<()>| async move {
    ///     while inbox.recv().await.is_some() {}
    ///     Ok(())
    /// })
    /// .unwrap();
    /// second.signal_shutdown();
    /// # }
    /// ```
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
            let span = tracing::debug_span!("process", name = %self.name());
            let inbox = Inbox::try_new(self.clone())?;
            let address = inbox.address().clone();
            let mut bomb = AbortBomb::new(address);
            bomb.address
                ._channel()
                .register_spawned()
                .expect("Transition must succeed, because inbox was just created");
            let spawn_future = AssertUnwindSafe(spawn_fn(inbox)).catch_unwind();

            NAME_INFO
                .scope(
                    NameInfo {
                        this: self.name().clone(),
                        parent: current_name(),
                    },
                    async move {
                        let spawn_result = spawn_future.await;

                        let mapped_result = match spawn_result {
                            Ok(result) => {
                                match &result {
                                    Ok(_) => bomb.address._channel().register_exited(Ok(())),
                                    Err(_) => bomb
                                        .address
                                        ._channel()
                                        .register_exited(Err(ExitError::UnhandledError)),
                                };

                                result
                            }

                            Err(boxed) => {
                                bomb.address
                                    ._channel()
                                    .register_exited(Err(ExitError::Panicked));
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
                self.address
                    ._channel()
                    .register_exited(Err(ExitError::Aborted));
            }
        }
    }
}

pub(crate) fn current_name() -> Option<Name> {
    NAME_INFO.try_with(|info| info.this.clone()).ok()
}

#[expect(unused)]
pub(crate) fn parent_name() -> Option<Name> {
    NAME_INFO
        .try_with(|info| info.parent.clone())
        .ok()
        .flatten()
}
