use crate::*;
use futures::{Future, FutureExt, Stream};
use std::{
    mem::{self},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

/// A child is a unique reference to an actor, consisting of one or more processes.
/// ...
/// ...
/// ...
///
/// # Generics
///
/// A `Child<E, CT, PT>` has the following generics:
/// - `E`: Stands for `Exit`, and is the value that the spawned child exits with.
/// - `C`: Stands for [`DefinesChannel`], and specifies the underlying channel and what kind of
///   messages the actor [trait@Accepts] (the default is [`Accepts![]`](Accepts!)). This value can be
///   one of the following:
///     - The [`Protocol`] of the actor, given that the actor has an [Inbox].
///     - [`Halter`], indicating that the actor does not have an [Inbox].
///     - A dynamic child using the [`Accepts!`] macro. This can be used for actors with or without
///       an [Inbox], and actors of either type can be converted using [`Child::transform_into`].
/// - `P`: Stands for [`DefinesPool`], and specifies whether this child is pooled or not (default
///   is [NoPool]). This can be either:
///     - [NoPool], indicating the child is not pooled.
///     - [Pool], indicating that the child is a [ChildPool].
///
/// ## Shorthands
/// In order to keep writing these types simple, we can use the following shorthands to define our
/// types:
/// ```no_compile
/// Child<E, C>     = Child<E, C, NoPool>;       // A regular child.
/// Child<E>        = Child<E, Accepts![]>;      // A child that doesn't accept any messages.
/// ChildPool<E, C> = Child<E, C, Pool>;         // A regular childpool.
/// ChildPool<E>    = ChildPool<E, Accepts![]>;  // A childpool that doesn't accept any messages.
/// ```
#[derive(Debug)]
pub struct Child<E, C = Accepts![], P = NoPool>
where
    E: Send + 'static,
    C: DefinesChannel,
    P: DefinesPool,
{
    channel: Arc<C::Channel>,
    join_handles: Option<P::JoinHandles<E>>,
    link: Link,
    is_aborted: bool,
}

//-------------------------------------------------
//  All children
//-------------------------------------------------

/// # Methods valid for all children.
impl<E, C, P> Child<E, C, P>
where
    E: Send + 'static,
    C: DefinesChannel,
    P: DefinesPool,
{
    pub(crate) fn new(
        channel: Arc<C::Channel>,
        join_handle: P::JoinHandles<E>,
        link: Link,
    ) -> Self {
        Self {
            join_handles: Some(join_handle),
            link,
            channel,
            is_aborted: false,
        }
    }

    fn into_parts(self) -> (Arc<C::Channel>, Option<P::JoinHandles<E>>, Link, bool) {
        let no_drop = mem::ManuallyDrop::new(self);
        unsafe {
            let handle = std::ptr::read(&no_drop.join_handles);
            let channel = std::ptr::read(&no_drop.channel);
            let link = std::ptr::read(&no_drop.link);
            let is_aborted = std::ptr::read(&no_drop.is_aborted);
            (channel, handle, link, is_aborted)
        }
    }

    /// Get the underlying [`tokio::task::JoinHandle`].
    ///
    /// This will not run the drop, and therefore the actor will not be halted/aborted.
    pub fn into_inner(self) -> P::JoinHandles<E> {
        self.into_parts().1.take().unwrap()
    }

    /// Get a new [Address] to the [Channel].
    pub fn get_address(&self) -> Address<C> {
        self.channel.add_address();
        Address::new(self.channel.clone())
    }

    /// Attach the actor.
    ///
    /// Returns the old abort-timeout if it was already attached.
    pub fn attach(&mut self, duration: Duration) -> Option<Duration> {
        self.link.attach(duration)
    }

    /// Detach the actor.
    ///
    /// Returns the old abort-timeout if it was attached before.
    pub fn detach(&mut self) -> Option<Duration> {
        self.link.detach()
    }

    /// Returns true if the actor has been aborted. This does not mean that the tasks have exited,
    /// but only that aborting has begun.
    pub fn is_aborted(&self) -> bool {
        self.is_aborted
    }

    /// Whether the actor is attached.
    pub fn is_attached(&self) -> bool {
        self.link.is_attached()
    }

    /// Get a reference to the current [Link] of the actor.
    pub fn link(&self) -> &Link {
        &self.link
    }

    pub fn config(&self) -> Config {
        Config {
            link: self.link().clone(),
            capacity: self.capacity().clone(),
        }
    }

    pub fn abort(&mut self) -> bool {
        self.channel.close();
        let was_aborted = self.is_aborted;
        self.is_aborted = true;
        P::abort(self.join_handles.as_ref().unwrap());
        !was_aborted
    }

    /// Whether the task is finished.
    pub fn is_finished(&self) -> bool {
        P::is_finished(self.join_handles.as_ref().unwrap())
    }

    pub fn transform_unchecked_into<T>(self) -> Child<E, T, P>
    where
        T: DefinesDynChannel,
    {
        let (channel, join_handles, link, is_aborted) = self.into_parts();
        Child {
            join_handles,
            channel: C::into_dyn_channel(channel),
            link,
            is_aborted,
        }
    }

    pub fn transform_into<T>(self) -> Child<E, T, P>
    where
        C: TransformInto<T>,
        T: DefinesDynChannel,
    {
        let (channel, join_handles, link, is_aborted) = self.into_parts();
        Child {
            join_handles,
            channel: C::transform_into(channel),
            link,
            is_aborted,
        }
    }

    pub fn into_dyn(self) -> Child<E, Accepts!(), P> {
        self.transform_unchecked_into()
    }

    gen::channel_methods!();
    gen::send_methods!(C);
}

//-------------------------------------------------
//  Dynamic children
//-------------------------------------------------

/// # Methods valid for children that are dynamic.
impl<E, C, P> Child<E, C, P>
where
    E: Send + 'static,
    C: DefinesDynChannel,
    P: DefinesPool,
{
    pub fn try_transform<T>(self) -> Result<Child<E, T, P>, Self>
    where
        T: DefinesDynChannel,
    {
        if T::msg_ids().iter().all(|id| self.accepts(id)) {
            Ok(self.transform_unchecked_into())
        } else {
            Err(self)
        }
    }

    pub fn downcast<T>(self) -> Result<Child<E, T, P>, Self>
    where
        T: DefinesChannel,
        T::Channel: Sized + 'static,
    {
        let (channel, join_handles, link, is_aborted) = self.into_parts();
        match channel.clone().into_any().downcast() {
            Ok(channel) => Ok(Child {
                join_handles,
                channel,
                link,
                is_aborted,
            }),
            Err(_) => Err(Child {
                join_handles,
                channel,
                link,
                is_aborted,
            }),
        }
    }

    /// Whether the actor accepts a message of the given type-id.
    pub fn accepts(&self, id: &std::any::TypeId) -> bool {
        self.channel.accepts(id)
    }

    gen::unchecked_send_methods!();
}

//-------------------------------------------------
//  Non-pooled children
//-------------------------------------------------

/// # Methods valid for children that are not pooled.
impl<E, C> Child<E, C, NoPool>
where
    E: Send + 'static,
    C: DefinesChannel,
{
    /// Convert the [Child] into a [ChildPool].
    pub fn into_pool(self) -> ChildPool<E, C> {
        let (channel, mut join_handles, link, is_aborted) = self.into_parts();
        ChildPool {
            channel,
            join_handles: Some(vec![join_handles.take().unwrap()]),
            link,
            is_aborted,
        }
    }

    /// Halts the actor, and then waits for it to exit. This always returns with the
    /// result of the task, and closes the channel.
    ///
    /// If the timeout expires before the actor has exited, the actor will be aborted.
    pub fn shutdown(&mut self, timeout: Duration) -> ShutdownFut<'_, E, C> {
        ShutdownFut::new(self, timeout)
    }
}

//-------------------------------------------------
//  Pooled children
//-------------------------------------------------

/// # Methods valid for children that are pooled.
impl<E, C> Child<E, C, Pool>
where
    E: Send + 'static,
    C: DefinesChannel,
{
    /// The amount of tasks that are alive.
    ///
    /// This should give the same result as [ChildPool::process_count], as long as
    /// an inbox is only dropped whenever it's task finishes.
    pub fn task_count(&self) -> usize {
        self.join_handles
            .as_ref()
            .unwrap()
            .iter()
            .filter(|handle| !handle.is_finished())
            .collect::<Vec<_>>()
            .len()
    }

    /// The amount of handles to processes that this pool contains. This can be bigger
    /// than the `process_count` or `task_count` if processes have exited.
    pub fn handle_count(&self) -> usize {
        self.join_handles.as_ref().unwrap().len()
    }

    /// Attempt to spawn an additional process on the channel. // todo: make this work
    ///
    /// This method can fail if
    /// * the message-type does not match that of the channel.
    /// * the channel has already exited.
    pub fn try_spawn<I, Fun, Fut>(&mut self, fun: Fun) -> Result<(), TrySpawnError<Fun>>
    where
        Fun: FnOnce(I) -> Fut + Send + 'static,
        Fut: Future<Output = E> + Send + 'static,
        I: SpawnsWith,
        <I::ChannelDefinition as DefinesChannel>::Channel: Sized,
        E: Send + 'static,
    {
        let channel = match Arc::downcast::<<I::ChannelDefinition as DefinesChannel>::Channel>(
            self.channel.clone().into_any(),
        ) {
            Ok(channel) => channel,
            Err(_) => return Err(TrySpawnError::IncorrectType(fun)),
        };

        match channel.try_add_inbox() {
            Ok(_) => {
                let inbox = I::new(channel);
                let handle = tokio::task::spawn(async move { fun(inbox).await });
                self.join_handles.as_mut().unwrap().push(handle);
                Ok(())
            }
            Err(_) => Err(TrySpawnError::Exited(fun)),
        }
    }

    pub fn shutdown(&mut self, timeout: Duration) -> ShutdownStream<'_, E, C> {
        ShutdownStream::new(self, timeout)
    }
}

//-------------------------------------------------
//  (Sized and pooled) children
//-------------------------------------------------

/// # Methods valid for children that are both sized and pooled.
impl<E, C> Child<E, C, Pool>
where
    E: Send + 'static,
    C: DefinesSizedChannel,
    C::Channel: Sized,
{
    /// Attempt to spawn an additional process on the channel.
    ///
    /// This method fails if the channel has already exited.
    pub fn spawn<Fun, Fut>(&mut self, fun: Fun) -> Result<(), SpawnError<Fun>>
    where
        Fun: FnOnce(C::Spawned) -> Fut + Send + 'static,
        Fut: Future<Output = E> + Send + 'static,
        E: Send + 'static,
        C: Send + 'static,
    {
        match self.channel.try_add_inbox() {
            Ok(_) => {
                let inbox = <C::Spawned as SpawnsWith>::new(self.channel.clone());
                let handle = tokio::task::spawn(async move { fun(inbox).await });
                self.join_handles.as_mut().unwrap().push(handle);
                Ok(())
            }
            Err(_) => Err(SpawnError(fun)),
        }
    }
}

//-------------------------------------------------
//  Unpin for all children
//-------------------------------------------------

impl<E, C, P> Unpin for Child<E, C, P>
where
    E: Send + 'static,
    C: DefinesChannel,
    P: DefinesPool,
{
}

//-------------------------------------------------
//  Future for unpooled children
//-------------------------------------------------

impl<E, C> Future for Child<E, C, NoPool>
where
    E: Send + 'static,
    C: DefinesChannel,
{
    type Output = Result<E, ExitError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.join_handles
            .as_mut()
            .unwrap()
            .poll_unpin(cx)
            .map_err(|e| e.into())
    }
}

//-------------------------------------------------
//  Stream for pooled children
//-------------------------------------------------

impl<E, C> Stream for Child<E, C, Pool>
where
    E: Send + 'static,
    C: DefinesChannel,
{
    type Item = Result<E, ExitError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.join_handles.as_ref().unwrap().len() == 0 {
            return Poll::Ready(None);
        }

        for (i, handle) in self.join_handles.as_mut().unwrap().iter_mut().enumerate() {
            if let Poll::Ready(res) = handle.poll_unpin(cx) {
                self.join_handles.as_mut().unwrap().swap_remove(i);
                return Poll::Ready(Some(res.map_err(Into::into)));
            }
        }

        Poll::Pending
    }
}

//-------------------------------------------------
//  Drop for all children
//-------------------------------------------------

impl<E, C, P> Drop for Child<E, C, P>
where
    E: Send + 'static,
    C: DefinesChannel,
    P: DefinesPool,
{
    fn drop(&mut self) {
        if let Link::Attached(abort_timer) = self.link {
            if !self.is_aborted && !self.is_finished() {
                if abort_timer.is_zero() {
                    self.abort();
                } else {
                    self.halt();
                    let handles = self.join_handles.take().unwrap();
                    tokio::task::spawn(async move {
                        tokio::time::sleep(abort_timer).await;
                        P::abort(&handles)
                    });
                }
            }
        }
    }
}

//------------------------------------------------------------------------------------------------
//  ChildPool
//------------------------------------------------------------------------------------------------

/// A `ChildPool<E, CT>` is the same as a `Child<E, CT, Pool>`.
pub type ChildPool<E, CT = Accepts![]> = Child<E, CT, Pool>;

//------------------------------------------------------------------------------------------------
//  IntoChild
//------------------------------------------------------------------------------------------------

pub trait IntoChild<E, T, R>
where
    E: Send + 'static,
    T: DefinesChannel,
    R: DefinesPool,
{
    fn into_child(self) -> Child<E, T, R>;
}

impl<E, P, T2> IntoChild<E, T2, NoPool> for Child<E, P>
where
    E: Send + 'static,
    P: Protocol + TransformInto<T2>,
    T2: DefinesDynChannel,
{
    fn into_child(self) -> Child<E, T2> {
        self.transform_into()
    }
}

impl<E, P, T2> IntoChild<E, T2, Pool> for Child<E, P>
where
    E: Send + 'static,
    P: Protocol + TransformInto<T2>,
    T2: DefinesDynChannel,
{
    fn into_child(self) -> ChildPool<E, T2> {
        self.into_pool().transform_into()
    }
}

impl<E, D, T2> IntoChild<E, T2, NoPool> for Child<E, Dyn<D>>
where
    E: Send + 'static,
    T2: DefinesDynChannel,
    D: ?Sized,
    Dyn<D>: DefinesDynChannel + TransformInto<T2>,
{
    fn into_child(self) -> Child<E, T2> {
        self.transform_into()
    }
}

impl<E, D, T2> IntoChild<E, T2, Pool> for Child<E, Dyn<D>>
where
    E: Send + 'static,
    T2: DefinesDynChannel,
    D: ?Sized,
    Dyn<D>: DefinesDynChannel + TransformInto<T2>,
{
    fn into_child(self) -> ChildPool<E, T2> {
        self.into_pool().transform_into()
    }
}

impl<E, D, T2> IntoChild<E, T2, Pool> for ChildPool<E, Dyn<D>>
where
    E: Send + 'static,
    T2: DefinesDynChannel,
    D: ?Sized,
    Dyn<D>: DefinesDynChannel + TransformInto<T2>,
{
    fn into_child(self) -> ChildPool<E, T2> {
        self.transform_into()
    }
}

impl<E, P, T2> IntoChild<E, T2, Pool> for ChildPool<E, P>
where
    E: Send + 'static,
    P: Protocol + TransformInto<T2>,
    T2: DefinesDynChannel,
{
    fn into_child(self) -> ChildPool<E, T2> {
        self.transform_into()
    }
}

//------------------------------------------------------------------------------------------------
//  Test
//------------------------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use crate::*;
    use std::{future::pending, time::Duration};
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn drop_halts_single_actor() {
        let (tx, rx) = oneshot::channel();
        let (child, _addr) = spawn(Config::default(), |mut inbox: Inbox<()>| async move {
            if let Err(RecvError::Halted) = inbox.recv().await {
                tx.send(true).unwrap();
            } else {
                tx.send(false).unwrap()
            }
        });
        drop(child);
        assert!(rx.await.unwrap());
    }

    #[tokio::test]
    async fn dropping_aborts() {
        let (tx, rx) = oneshot::channel();
        let (child, _addr) = spawn(
            Config::attached(Duration::from_millis(1)),
            |mut inbox: Inbox<()>| async move {
                if let Err(RecvError::Halted) = inbox.recv().await {
                    tx.send(true).unwrap();
                    pending::<()>().await;
                } else {
                    tx.send(false).unwrap()
                }
            },
        );
        drop(child);
        assert!(rx.await.unwrap());
    }

    #[tokio::test]
    async fn dropping_detached() {
        let (tx, rx) = oneshot::channel();
        let (child, addr) = spawn(Config::detached(), |mut inbox: Inbox<()>| async move {
            if let Err(RecvError::Halted) = inbox.recv().await {
                tx.send(true).unwrap();
            } else {
                tx.send(false).unwrap()
            }
        });
        drop(child);
        tokio::time::sleep(Duration::from_millis(1)).await;
        addr.try_send(()).unwrap();
        assert!(!rx.await.unwrap());
    }

    #[tokio::test]
    async fn downcast() {
        let (child, _addr) = spawn(Config::default(), basic_actor!());
        assert!(matches!(
            child.transform_into::<Accepts![]>().downcast::<()>(),
            Ok(_)
        ));
    }

    #[tokio::test]
    async fn abort() {
        let (mut child, _addr) = spawn(Config::default(), basic_actor!());
        assert!(!child.is_aborted());
        child.abort();
        assert!(child.is_aborted());
        assert!(matches!(child.await, Err(ExitError::Abort)));
    }

    #[tokio::test]
    async fn is_finished() {
        let (mut child, _addr) = spawn(Config::default(), basic_actor!());
        child.abort();
        let _ = (&mut child).await;
        assert!(child.is_finished());
    }

    #[tokio::test]
    async fn into_childpool() {
        let (child, _addr) = spawn(Config::default(), basic_actor!());
        let pool = child.into_pool();
        assert_eq!(pool.task_count(), 1);
        assert_eq!(pool.process_count(), 1);
        assert_eq!(pool.is_aborted(), false);

        let (mut child, _addr) = spawn(Config::default(), basic_actor!());
        child.abort();
        let pool = child.into_pool();
        assert_eq!(pool.is_aborted(), true);
    }
}

#[cfg(test)]
mod test_pooled {
    use crate::*;
    use futures::future::pending;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn dropping() {
        static HALT_COUNT: AtomicU8 = AtomicU8::new(0);
        let (child, addr) = spawn_many(
            0..3,
            Config::default(),
            |_, mut inbox: Inbox<()>| async move {
                if let Err(RecvError::Halted) = inbox.recv().await {
                    HALT_COUNT.fetch_add(1, Ordering::AcqRel);
                };
            },
        );
        drop(child);
        addr.await;

        assert_eq!(HALT_COUNT.load(Ordering::Acquire), 3);
    }

    #[tokio::test]
    async fn dropping_halts_then_aborts() {
        static HALT_COUNT: AtomicU8 = AtomicU8::new(0);
        let (child, addr) = spawn_many(
            0..3,
            Config::attached(Duration::from_millis(1)),
            |_, mut inbox: Inbox<()>| async move {
                if let Err(RecvError::Halted) = inbox.recv().await {
                    HALT_COUNT.fetch_add(1, Ordering::AcqRel);
                };
                pending::<()>().await;
            },
        );
        drop(child);
        addr.await;

        assert_eq!(HALT_COUNT.load(Ordering::Acquire), 3);
    }

    #[tokio::test]
    async fn dropping_detached() {
        static HALT_COUNT: AtomicU8 = AtomicU8::new(0);

        let (child, addr) = spawn_many(
            0..3,
            Config::detached(),
            |_, mut inbox: Inbox<()>| async move {
                if let Err(RecvError::Halted) = inbox.recv().await {
                    HALT_COUNT.fetch_add(1, Ordering::AcqRel);
                };
            },
        );
        drop(child);
        tokio::time::sleep(Duration::from_millis(1)).await;
        addr.try_send(()).unwrap();
        addr.try_send(()).unwrap();
        addr.try_send(()).unwrap();
        addr.await;

        assert_eq!(HALT_COUNT.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn downcast() {
        let (pool, _addr) = spawn_many(0..5, Config::default(), pooled_basic_actor!());
        let pool: ChildPool<_> = pool.transform_into();
        assert!(matches!(pool.downcast::<()>(), Ok(_)));
    }

    #[tokio::test]
    async fn spawn_ok() {
        let (mut child, _addr) = spawn_one(Config::default(), basic_actor!());
        assert!(child.spawn(basic_actor!()).is_ok());
        assert!(child
            .transform_into::<Accepts![]>()
            .try_spawn(basic_actor!())
            .is_ok());
    }

    #[tokio::test]
    async fn spawn_err_exit() {
        let (mut child, addr) = spawn_one(Config::default(), basic_actor!());
        addr.halt();
        addr.await;
        assert!(matches!(child.spawn(basic_actor!()), Err(SpawnError(_))));
        assert!(matches!(
            child
                .transform_into::<Accepts![]>()
                .try_spawn(basic_actor!()),
            Err(TrySpawnError::Exited(_))
        ));
    }

    #[tokio::test]
    async fn spawn_err_incorrect_type() {
        let (child, _addr) = spawn_one(Config::default(), basic_actor!(U32Protocol));
        assert!(matches!(
            child
                .transform_into::<Accepts![]>()
                .try_spawn(basic_actor!(())),
            Err(TrySpawnError::IncorrectType(_))
        ));
    }
}
