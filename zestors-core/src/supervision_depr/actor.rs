use std::convert::Infallible;

use crate::_prelude::*;

pub trait Actor: Send + Sized + 'static {
    type Interface: Interface;
    type Exit: Send + 'static;

    fn run(
        self,
        state: Inbox<Self::Interface>,
    ) -> impl Future<Output = Result<Self::Exit, Report>> + Send + 'static;
}

pub trait ActorExt: Actor {
    fn map_actor_exit<F, R>(self, map_exit: F) -> MapActor<Self, F>
    where
        F: FnOnce(Result<Self::Exit, Report>) -> Result<R, Report> + Send + 'static,
        R: Send + 'static,
    {
        MapActor::new(self, map_exit)
    }

    fn wrap_actor<F, Fut, E>(self, mapper: F) -> WrapActor<Self, F>
    where
        F: FnOnce(Self, Inbox<Self::Interface>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<E, Report>> + Send + 'static,
        E: Send + 'static,
    {
        WrapActor::new(self, mapper)
    }

    fn spawn_with(self, pid: Pid) -> Result<Child<Self::Exit, Self::Interface>, DuplicatePidError> {
        crate::spawn_with(pid, |inbox| self.run(inbox))
    }

    fn spawn(self) -> Child<Self::Exit, Self::Interface> {
        crate::spawn(|inbox| self.run(inbox))
    }
}
impl<T: Actor> ActorExt for T {}

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

pub fn actor_fn<F, Fut, I, E>(f: F) -> FnActor<F, Fut, I, E>
where
    F: FnOnce(Inbox<I>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<E, Report>> + Send + 'static,
    I: Interface,
    E: Send + 'static,
{
    FnActor::new(f)
}

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

pub fn task_fn<F, Fut, E>(f: F) -> FnTask<F, Fut, E>
where
    F: FnOnce(TaskBox) -> Fut + Send + 'static,
    Fut: Future<Output = Result<E, Report>> + Send + 'static,
    E: Send + 'static,
{
    FnTask::new(f)
}
