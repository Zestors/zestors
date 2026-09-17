use rootcause::Report;
use std::{convert::Infallible, fmt::Debug};
use zestors_interface::Interface;
use zestors_runtime::{
    prelude::*,
    spawn_rand,
    {TaskBox, errors::DuplicatePidError},
};

/// The core trait for an actor's behavior: given an [`Inbox`], runs until it exits.
///
/// Most actors are more conveniently implemented via a [`Handler`](crate::Handler),
/// which implements `Actor` automatically. Implement `Actor` directly for full
/// control over the actor's event loop.
pub trait Actor: Send + Sized + 'static {
    /// The [`Interface`] of messages this actor accepts.
    type Interface: Interface;

    /// The value produced once this actor's [`Actor::run`] completes.
    type Exit: Send + 'static;

    /// Runs the actor to completion, receiving messages and signals through `inbox`.
    fn run(
        self,
        inbox: Inbox<Self::Interface>,
    ) -> impl Future<Output = Result<Self::Exit, Report>> + Send + 'static;
}

/// Combinators for adapting an [`Actor`]'s behavior, and for spawning it.
/// Implemented automatically for every [`Actor`].
pub trait ActorExt: Actor {
    /// Wraps this actor so that `map_exit` transforms its exit value once
    /// [`Actor::run`] completes.
    fn map_actor_exit<F, R>(self, map_exit: F) -> MapActor<Self, F>
    where
        F: FnOnce(Result<Self::Exit, Report>) -> Result<R, Report> + Send + 'static,
        R: Send + 'static,
    {
        MapActor::new(self, map_exit)
    }

    /// Wraps this actor so that `mapper` runs instead of [`Actor::run`],
    /// given the original actor and its [`Inbox`].
    fn wrap_actor<F, Fut, E>(self, mapper: F) -> WrapActor<Self, F>
    where
        F: FnOnce(Self, Inbox<Self::Interface>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        E: Send + 'static,
    {
        WrapActor::new(self, mapper)
    }

    /// Spawns this actor under a specific [`Pid`], returning a [`Child`] that
    /// owns its task. Fails if `pid` is already registered.
    fn spawn_with(self, pid: Pid) -> Result<Child<Self::Exit, Self::Interface>, DuplicatePidError> {
        spawn(pid, |inbox| self.run(inbox))
    }

    /// Spawns this actor under a freshly generated [`Pid`], returning a
    /// [`Child`] that owns its task.
    fn spawn(self) -> Child<Self::Exit, Self::Interface> {
        spawn_rand(|inbox| self.run(inbox))
    }
}
impl<T: Actor> ActorExt for T {}

/// Creates a signal-only [`Actor`] (no messages, only [`Signal`]s) from `f`,
/// which receives a [`TaskBox`] instead of a typed [`Inbox`].
pub fn fn_task<F, Fut, E>(f: F) -> FnTask<F, Fut, E>
where
    F: FnOnce(TaskBox) -> Fut + Send + 'static,
    Fut: Future<Output = Result<E, Report>> + Send + 'static,
    E: Send + 'static,
{
    FnTask::new(f)
}

/// Creates an [`Actor`] from `f`, which receives the actor's [`Inbox`] directly.
pub fn fn_actor<F, Fut, I, E>(f: F) -> FnActor<F, Fut, I, E>
where
    F: FnOnce(Inbox<I>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<E, Report>> + Send + 'static,
    I: Interface,
    E: Send + 'static,
{
    FnActor::new(f)
}

mod _hidden {
    use super::*;

    /// The [`Actor`] returned by [`ActorExt::map_actor_exit`].
    #[derive(Clone)]
    pub struct MapActor<T, F> {
        inner: T,
        map_exit: F,
    }

    impl<T, F> Debug for MapActor<T, F>
    where
        T: Debug,
    {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("MapRun")
                .field("inner", &self.inner)
                .finish()
        }
    }

    impl<T, F> MapActor<T, F> {
        pub fn new<R>(inner: T, map_exit: F) -> Self
        where
            T: Actor,
            F: FnOnce(Result<T::Exit, Report>) -> Result<R, Report> + Send + 'static,
            R: Send + 'static,
        {
            Self { inner, map_exit }
        }
    }

    impl<T, F, R> Actor for MapActor<T, F>
    where
        T: Actor + Send + 'static,
        F: FnOnce(Result<T::Exit, Report>) -> Result<R, Report> + Send + 'static,
        R: Send + 'static,
    {
        type Interface = T::Interface;
        type Exit = R;

        fn run(
            self,
            state: Inbox<Self::Interface>,
        ) -> impl Future<Output = Result<Self::Exit, Report>> + Send + 'static {
            let Self { inner, map_exit } = self;

            async move { map_exit(inner.run(state).await) }
        }
    }

    /// The [`Actor`] returned by [`ActorExt::wrap_actor`].
    #[derive(Clone)]
    pub struct WrapActor<T, F> {
        inner: T,
        mapper: F,
    }

    impl<T, F> Debug for WrapActor<T, F>
    where
        T: Debug,
    {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WrapRun")
                .field("inner", &self.inner)
                .finish()
        }
    }

    impl<T, F> WrapActor<T, F> {
        pub fn new<R, Fut>(inner: T, mapper: F) -> Self
        where
            T: Actor,
            F: FnOnce(T, Inbox<T::Interface>) -> Fut + Send + 'static,
            Fut: Future<Output = Result<R, Report>> + Send + 'static,
            R: Send + 'static,
        {
            Self { inner, mapper }
        }
    }

    impl<T, F, Fut, E> Actor for WrapActor<T, F>
    where
        T: Actor + Send + 'static,
        F: FnOnce(T, Inbox<T::Interface>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        E: Send + 'static,
    {
        type Interface = T::Interface;
        type Exit = E;

        fn run(
            self,
            state: Inbox<Self::Interface>,
        ) -> impl Future<Output = Result<Self::Exit, Report>> + Send + 'static {
            let Self { inner, mapper } = self;

            async move { mapper(inner, state).await }
        }
    }

    /// The [`Actor`] returned by [`fn_actor`](crate::fn_actor).
    pub struct FnActor<F, Fut, I, E>
    where
        F: FnOnce(Inbox<I>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        I: Interface,
        E: Send + 'static,
    {
        f: F,
        _phantom: std::marker::PhantomData<fn() -> (I, E, Fut)>,
    }

    impl<F, Fut, I, E> FnActor<F, Fut, I, E>
    where
        F: FnOnce(Inbox<I>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        I: Interface,
        E: Send + 'static,
    {
        pub fn new(f: F) -> Self {
            Self {
                f,
                _phantom: std::marker::PhantomData,
            }
        }
    }

    impl<F, Fut, I, E> Actor for FnActor<F, Fut, I, E>
    where
        F: FnOnce(Inbox<I>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        I: Interface,
        E: Send + 'static,
    {
        type Interface = I;
        type Exit = E;

        fn run(
            self,
            state: Inbox<Self::Interface>,
        ) -> impl Future<Output = Result<Self::Exit, Report>> + Send + 'static {
            (self.f)(state)
        }
    }

    impl<F, Fut, I, E> Debug for FnActor<F, Fut, I, E>
    where
        F: FnOnce(Inbox<I>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        I: Interface,
        E: Send + 'static,
    {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("FnActor")
                .field("interface", &std::any::type_name::<I>())
                .field("exit", &std::any::type_name::<E>())
                .finish()
        }
    }

    impl<F, Fut, I, E> Clone for FnActor<F, Fut, I, E>
    where
        F: Clone + FnOnce(Inbox<I>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        I: Interface,
        E: Send + 'static,
    {
        fn clone(&self) -> Self {
            Self {
                f: self.f.clone(),
                _phantom: std::marker::PhantomData,
            }
        }
    }

    /// The [`Actor`] returned by [`fn_task`](crate::fn_task).
    pub struct FnTask<F, Fut, E>
    where
        F: FnOnce(TaskBox) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        E: Send + 'static,
    {
        f: F,
        _phantom: std::marker::PhantomData<fn() -> (E, Fut)>,
    }

    impl<F, Fut, E> FnTask<F, Fut, E>
    where
        F: FnOnce(TaskBox) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        E: Send + 'static,
    {
        pub fn new(f: F) -> Self {
            Self {
                f,
                _phantom: std::marker::PhantomData,
            }
        }
    }

    impl<F, Fut, E> Actor for FnTask<F, Fut, E>
    where
        F: FnOnce(TaskBox) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        E: Send + 'static,
    {
        type Interface = Infallible;
        type Exit = E;

        fn run(
            self,
            state: Inbox<Self::Interface>,
        ) -> impl Future<Output = Result<Self::Exit, Report>> + Send + 'static {
            (self.f)(state.into_task_box())
        }
    }

    impl<F, Fut, E> Debug for FnTask<F, Fut, E>
    where
        F: FnOnce(TaskBox) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        E: Send + 'static,
    {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("FnTask")
                .field("exit", &std::any::type_name::<E>())
                .finish()
        }
    }

    impl<F, Fut, E> Clone for FnTask<F, Fut, E>
    where
        F: Clone + FnOnce(TaskBox) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        E: Send + 'static,
    {
        fn clone(&self) -> Self {
            Self {
                f: self.f.clone(),
                _phantom: std::marker::PhantomData,
            }
        }
    }
}
use _hidden::*;
