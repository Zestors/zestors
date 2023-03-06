use crate::all::*;
use futures::{Future, FutureExt, Stream};
use std::{
    mem,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll}, time::Duration,
};

/// A child is a unique [reference](ActorRef) to an actor similar to a [`tokio::task::JoinHandle`]. The
/// child can be used to halt/abort the actor and to monitor it's exit by awaiting it. When a child is dropped,
/// the child will (by default) be shut-down as well.
///
/// A lot of methods are located in the traits [`ActorRefExt`] and [`Transformable`].
/// 
/// # Monitoring
/// An actor can be monitored using it's [`Child`]. For a [`SingleProcess`]-child, this can be done directly
/// by awaiting the [`Child`], and returns an [`Result<E, ExitError>`](ExitError). A [`MultiProcess`]-child
/// can be [`streamed`](Stream) instead.
///
/// # Generics
/// The [`Child<E, A, C>`] is specified by:
/// - `E`: Stands for `Exit` and is the value that the spawned child exits with.
/// - `A`: Stands for [`ActorType`] and specifies the type of the actor.
/// - `C`: Stands for [`ChildType`] and specifies whether this child is pooled or not.
/// This can either be [`SingleProcess`] or [`MultiProcess`].
///
/// The following shorthand notation can be used:
/// ```no_compile
/// Child<E, A>     = Child<E, A, SingleProcess>;
/// Child<E>        = Child<E, DynActor!(), SingleProcess>;
/// ChildPool<E, A> = Child<E, A, MultiProcess>;
/// ChildPool<E>    = Child<E, DynActor!(), MultiProcess>;
/// ```
#[derive(Debug)]
#[must_use = "Dropping a child shuts down the actor!"]
pub struct Child<E, A = DynActor!(), C = SingleProcess>
where
    E: Send + 'static,
    A: ActorType,
    C: ChildType,
{
    channel: Arc<A::Channel>,
    join_handles: Option<C::JoinHandles<E>>,
    link: Link,
    is_aborted: bool,
}

/// Type-alias for child-pools, see [`Child`] for usage.
pub type ChildPool<E, A = DynActor!()> = Child<E, A, MultiProcess>;

/// # Methods valid for any child.
impl<E, A, C> Child<E, A, C>
where
    E: Send + 'static,
    A: ActorType,
    C: ChildType,
{
    pub(crate) fn new(
        channel: Arc<A::Channel>,
        join_handle: C::JoinHandles<E>,
        link: Link,
    ) -> Self {
        Self {
            join_handles: Some(join_handle),
            link,
            channel,
            is_aborted: false,
        }
    }

    fn into_parts(self) -> (Arc<A::Channel>, Option<C::JoinHandles<E>>, Link, bool) {
        let no_drop = mem::ManuallyDrop::new(self);
        unsafe {
            let handle = std::ptr::read(&no_drop.join_handles);
            let channel = std::ptr::read(&no_drop.channel);
            let link = std::ptr::read(&no_drop.link);
            let is_aborted = std::ptr::read(&no_drop.is_aborted);
            (channel, handle, link, is_aborted)
        }
    }

    /// Get the underlying [`tokio::task::JoinHandle`]s.
    ///
    /// # Warning
    /// This will not run the drop implementation and therefore the actor will not be halted/aborted.
    pub fn into_join_handles(self) -> C::JoinHandles<E> {
        self.into_parts().1.take().unwrap()
    }

    /// Attach the actor.
    ///
    /// Returns the old shutdown-ime if it was already attached.
    pub fn attach(&mut self, duration: Duration) -> Option<Duration> {
        self.link.attach(duration)
    }

    /// Detach the actor.
    ///
    /// Returns the old shutdown-time if it was attached.
    pub fn detach(&mut self) -> Option<Duration> {
        self.link.detach()
    }

    /// Returns true if the actor has been aborted. (see [`tokio::task::JoinHandle::abort`])
    ///
    /// This does not mean that the tasks have exited but only that aborting has begun.
    pub fn is_aborted(&self) -> bool {
        self.is_aborted
    }

    /// Whether the actor is attached.
    pub fn is_attached(&self) -> bool {
        self.link.is_attached()
    }

    /// Get a reference to the [`Link`] state.
    pub fn link(&self) -> &Link {
        &self.link
    }

    /// Aborts the actor. (see [`tokio::task::JoinHandle::abort`])
    ///
    /// Returns `true` if this is the first time aborting.
    pub fn abort(&mut self) -> bool {
        self.channel.close();
        let was_aborted = self.is_aborted;
        self.is_aborted = true;
        C::abort(self.join_handles.as_ref().unwrap());
        !was_aborted
    }

    /// Whether the tokio-tasks are finished. (see [`tokio::task::JoinHandle::is_finished`])
    pub fn is_finished(&self) -> bool {
        C::is_finished(self.join_handles.as_ref().unwrap())
    }
}

/// # Methods valid for single-process children only.
impl<E, A> Child<E, A, SingleProcess>
where
    E: Send + 'static,
    A: ActorType,
{
    /// Turn the [`Child`] into a [`ChildPool`].
    pub fn into_pool(self) -> ChildPool<E, A>
    where
        A: MultiProcessInbox,
    {
        let (channel, mut join_handles, link, is_aborted) = self.into_parts();
        ChildPool {
            channel,
            join_handles: Some(vec![join_handles.take().unwrap()]),
            link,
            is_aborted,
        }
    }

    /// Same as [`Self::shutdown`] but with a custom shutdown-time.
    pub fn shutdown_with(&mut self, time: Duration) -> ShutdownFut<'_, E, A> {
        ShutdownFut::new(self, time)
    }

    /// Halts the actor and then waits for it to exit. If the actor does not exit after the
    /// shutdown-time specified by the [`Link`] of this actor, the actor will be aborted.
    ///
    /// If the actor has a [`Link::Detached`], then [`get_default_shutdown_time`] is used.
    pub fn shutdown(&mut self) -> ShutdownFut<'_, E, A> {
        let duration = match self.link {
            Link::Detached => get_default_shutdown_time(),
            Link::Attached(duration) => duration,
        };
        self.shutdown_with(duration)
    }
}

/// # Methods valid for children multi-process children only.
impl<E, A> Child<E, A, MultiProcess>
where
    E: Send + 'static,
    A: ActorType,
{
    /// The amount of tokio-tasks that have not finished.
    pub fn task_count(&self) -> usize {
        self.join_handles
            .as_ref()
            .unwrap()
            .iter()
            .filter(|handle| !handle.is_finished())
            .collect::<Vec<_>>()
            .len()
    }

    /// The amount of tokio-tasks, including those that have finished.
    pub fn handle_count(&self) -> usize {
        self.join_handles.as_ref().unwrap().len()
    }

    /// Same as [`Self::shutdown`] but with a custom shutdown-time.
    pub fn shutdown_with(&mut self, time: Duration) -> ShutdownStream<'_, E, A> {
        ShutdownStream::new(self, time)
    }

    /// Halts the actor and then waits for all processes to exit. If the actor does not exit after the
    /// shutdown-time specified by the [`Link`] of this actor, the actor will be aborted.
    ///
    /// The returned value is a [`Stream`] that resolves one-by-one for every process.
    ///
    /// If the actor has a [`Link::Detached`], then [`get_default_shutdown_time`] is used.
    pub fn shutdown(&mut self) -> ShutdownStream<'_, E, A> {
        let duration = match self.link {
            Link::Detached => get_default_shutdown_time(),
            Link::Attached(duration) => duration,
        };
        self.shutdown_with(duration)
    }

    /// Attempt to spawn an additional process on the channel.
    ///
    /// This method fails if the actor has already exited.
    pub fn spawn_onto<Fun, Fut>(&mut self, fun: Fun) -> Result<(), SpawnError<Fun>>
    where
        Fun: FnOnce(A) -> Fut + Send + 'static,
        Fut: Future<Output = E> + Send + 'static,
        A: MultiProcessInbox,
    {
        match self.channel.try_increment_process_count() {
            Ok(_) => {
                let inbox = A::from_channel(self.channel.clone());
                let handle = tokio::task::spawn(async move { fun(inbox).await });
                self.join_handles.as_mut().unwrap().push(handle);
                Ok(())
            }
            Err(e) => {
                match e {
                    AddProcessError::ActorHasExited => Err(SpawnError(fun)),
                    AddProcessError::SingleProcessOnly => {
                        panic!("Error with implementation of the Inbox. This is a Bug, please report it.")
                    }
                }
            }
        }
    }

    /// Attempt to spawn an additional process onto the channel.
    ///
    /// This method can fail if
    /// - The [`ActorType`] `T` does not match that of the actor.
    /// - The actor has already exited.
    pub fn try_spawn_onto<T, Fun, Fut>(&mut self, fun: Fun) -> Result<(), TrySpawnError<Fun>>
    where
        Fun: FnOnce(T) -> Fut + Send + 'static,
        Fut: Future<Output = E> + Send + 'static,
        A: DynActorType,
        T: MultiProcessInbox,
        T::Channel: Sized,
    {
        let channel = match Arc::downcast::<T::Channel>(self.channel.clone().into_any()) {
            Ok(channel) => channel,
            Err(_) => return Err(TrySpawnError::WrongInbox(fun)),
        };

        match channel.try_increment_process_count() {
            Ok(_) => {
                let inbox = T::from_channel(channel);
                let handle = tokio::task::spawn(async move { fun(inbox).await });
                self.join_handles.as_mut().unwrap().push(handle);
                Ok(())
            }
            Err(e) => {
                match e {
                    AddProcessError::ActorHasExited => Err(TrySpawnError::Exited(fun)),
                    AddProcessError::SingleProcessOnly => {
                        panic!("Error with implementation of the Inbox. This is a Bug, please report it.")
                    }
                }
            }
        }
    }
}

impl<E, A, C> ActorRef for Child<E, A, C>
where
    E: Send + 'static,
    A: ActorType,
    C: ChildType,
{
    type ActorType = A;
    fn channel_ref(this: &Self) -> &Arc<<Self::ActorType as ActorType>::Channel> {
        &this.channel
    }
}

impl<E, A, C> Transformable for Child<E, A, C>
where
    E: Send + 'static,
    A: ActorType,
    C: ChildType,
{
    type IntoRef<T> = Child<E, T, C> where T: ActorType;

    fn transform_unchecked_into<T>(self) -> Self::IntoRef<T>
    where
        T: DynActorType,
    {
        let (channel, join_handles, link, is_aborted) = self.into_parts();
        Child {
            join_handles,
            channel: <A::Channel as Channel>::into_dyn(channel),
            link,
            is_aborted,
        }
    }

    fn transform_into<T>(self) -> Self::IntoRef<T>
    where
        Self::ActorType: TransformInto<T>,
        T: ActorType,
    {
        let (channel, join_handles, link, is_aborted) = self.into_parts();
        Child {
            join_handles,
            channel: A::transform_into(channel),
            link,
            is_aborted,
        }
    }

    fn downcast<T>(self) -> Result<Self::IntoRef<T>, Self>
    where
        Self::ActorType: DynActorType,
        T: ActorType,
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
}

impl<E, A, C> Unpin for Child<E, A, C>
where
    E: Send + 'static,
    A: ActorType,
    C: ChildType,
{
}

/// # Future is implemented for single-process children only.
impl<E, A> Future for Child<E, A, SingleProcess>
where
    E: Send + 'static,
    A: ActorType,
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

/// # Stream is implemented for multi-process children only.
impl<E, A> Stream for Child<E, A, MultiProcess>
where
    E: Send + 'static,
    A: ActorType,
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

impl<E, A, C> Drop for Child<E, A, C>
where
    E: Send + 'static,
    A: ActorType,
    C: ChildType,
{
    fn drop(&mut self) {
        if let Link::Attached(shutdown_time) = &mut self.link {
            let shutdown_time = shutdown_time.clone();
            if !self.is_aborted && !self.is_finished() {
                self.halt();
                let handles = self.join_handles.take().unwrap();
                tokio::task::spawn(async move {
                    tokio::time::sleep(shutdown_time.clone()).await;
                    C::abort(&handles)
                });
            }
        }
    }
}

//------------------------------------------------------------------------------------------------
//  IntoChild
//------------------------------------------------------------------------------------------------

/// Implemented for any type that can be transformed into a [`Child<E, A, C>`].
pub trait IntoChild<E, A = DynActor!(), C = SingleProcess>
where
    E: Send + 'static,
    A: ActorType,
    C: ChildType,
{
    fn into_child(self) -> Child<E, A, C>;
}

impl<E, T, A> IntoChild<E, T, SingleProcess> for Child<E, A, SingleProcess>
where
    E: Send + 'static,
    T: ActorType,
    A: TransformInto<T>,
{
    fn into_child(self) -> Child<E, T, SingleProcess> {
        self.transform_into()
    }
}

impl<E, T, A> IntoChild<E, T, MultiProcess> for Child<E, A, SingleProcess>
where
    E: Send + 'static,
    T: ActorType,
    A: TransformInto<T> + MultiProcessInbox,
{
    fn into_child(self) -> Child<E, T, MultiProcess> {
        self.into_pool().transform_into()
    }
}

impl<E, G, T> IntoChild<E, T, MultiProcess> for Child<E, G, MultiProcess>
where
    E: Send + 'static,
    T: ActorType,
    G: TransformInto<T>,
{
    fn into_child(self) -> Child<E, T, MultiProcess> {
        self.transform_into()
    }
}

//------------------------------------------------------------------------------------------------
//  Test
//------------------------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use crate::_test::basic_actor;
    use crate::all::*;
    use std::{future::pending, time::Duration};
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn drop_halts_single_actor() {
        let (tx, rx) = oneshot::channel();
        let (child, _addr) = spawn(|mut inbox: Inbox<()>| async move {
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
        let (child, _addr) = spawn_with(
            Link::Attached(Duration::from_millis(1).into()),
            Default::default(),
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
        let (child, addr) = spawn_with(
            Link::Detached,
            Capacity::default(),
            |mut inbox: Inbox<()>| async move {
                if let Err(RecvError::Halted) = inbox.recv().await {
                    tx.send(true).unwrap();
                } else {
                    tx.send(false).unwrap()
                }
            },
        );
        drop(child);
        tokio::time::sleep(Duration::from_millis(1)).await;
        addr.try_send(()).unwrap();
        assert!(!rx.await.unwrap());
    }

    #[tokio::test]
    async fn downcast() {
        let (child, _addr) = spawn(basic_actor!());
        assert!(matches!(
            child.transform_into::<DynActor!()>().downcast::<Inbox<()>>(),
            Ok(_)
        ));
    }

    #[tokio::test]
    async fn abort() {
        let (mut child, _addr) = spawn(basic_actor!());
        assert!(!child.is_aborted());
        child.abort();
        assert!(child.is_aborted());
        assert!(matches!(child.await, Err(ExitError::Abort)));
    }

    #[tokio::test]
    async fn is_finished() {
        let (mut child, _addr) = spawn(basic_actor!());
        child.abort();
        let _ = (&mut child).await;
        assert!(child.is_finished());
    }

    #[tokio::test]
    async fn into_childpool() {
        let (child, _addr) = spawn(basic_actor!());
        let pool = child.into_pool();
        assert_eq!(pool.task_count(), 1);
        assert_eq!(pool.process_count(), 1);
        assert_eq!(pool.is_aborted(), false);

        let (mut child, _addr) = spawn(basic_actor!());
        child.abort();
        let pool = child.into_pool();
        assert_eq!(pool.is_aborted(), true);
    }
}

#[cfg(test)]
mod test_pooled {
    use crate::_test::{basic_actor, pooled_basic_actor, U32Protocol};
    use crate::all::*;
    use futures::future::pending;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn dropping() {
        static HALT_COUNT: AtomicU8 = AtomicU8::new(0);
        let (child, addr) = spawn_many(0..3, |_, mut inbox: Inbox<()>| async move {
            if let Err(RecvError::Halted) = inbox.recv().await {
                HALT_COUNT.fetch_add(1, Ordering::AcqRel);
            };
        });
        drop(child);
        addr.await;

        assert_eq!(HALT_COUNT.load(Ordering::Acquire), 3);
    }

    #[tokio::test]
    async fn dropping_halts_then_aborts() {
        static HALT_COUNT: AtomicU8 = AtomicU8::new(0);
        let (child, addr) = spawn_many(0..3, |_, mut inbox: Inbox<()>| async move {
            if let Err(RecvError::Halted) = inbox.recv().await {
                HALT_COUNT.fetch_add(1, Ordering::AcqRel);
            };
            pending::<()>().await;
        });
        drop(child);
        addr.await;

        assert_eq!(HALT_COUNT.load(Ordering::Acquire), 3);
    }

    #[tokio::test]
    async fn dropping_detached() {
        static HALT_COUNT: AtomicU8 = AtomicU8::new(0);

        let (child, addr) = spawn_many_with(
            Link::Detached,
            Capacity::default(),
            0..3,
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
        let (pool, _addr) = spawn_many(0..5, pooled_basic_actor!());
        let pool: ChildPool<_> = pool.transform_into();
        assert!(matches!(pool.downcast::<Inbox<()>>(), Ok(_)));
    }

    #[tokio::test]
    async fn spawn_ok() {
        let (mut child, _addr) = spawn_many(0..1, pooled_basic_actor!());
        assert!(child.spawn_onto(basic_actor!()).is_ok());
        assert!(child
            .transform_into::<DynActor!()>()
            .try_spawn_onto(basic_actor!())
            .is_ok());
    }

    #[tokio::test]
    async fn spawn_err_exit() {
        let (mut child, addr) = spawn_many(0..1, pooled_basic_actor!());
        addr.halt();
        addr.await;
        assert!(matches!(
            child.spawn_onto(basic_actor!()),
            Err(SpawnError(_))
        ));
        assert!(matches!(
            child
                .transform_into::<DynActor!()>()
                .try_spawn_onto(basic_actor!()),
            Err(TrySpawnError::Exited(_))
        ));
    }

    #[tokio::test]
    async fn spawn_err_incorrect_type() {
        let (child, _addr) = spawn(basic_actor!(U32Protocol));
        assert!(matches!(
            child
                .into_pool()
                .transform_into::<DynActor!()>()
                .try_spawn_onto(basic_actor!(())),
            Err(TrySpawnError::WrongInbox(_))
        ));
    }
}
