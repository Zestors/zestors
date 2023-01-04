use crate::*;
use futures::{future::BoxFuture, pin_mut, sink::With, Future};
use std::{
    error::Error,
    future::{ready, Ready},
    marker::PhantomData,
};

//------------------------------------------------------------------------------------------------
//  SpecifiesChild
//------------------------------------------------------------------------------------------------

pub trait SpecifiesChild<W = Self>:
    StartableWith<W, Output = (Child<Self::Exit, Self::ActorType>, Self::Restarter)>
    + Sized
    + Send
    + 'static
{
    type Exit: Send + 'static;
    type ActorType: ActorType;
    type Restarter: Restartable<Exit = Self::Exit, ActorType = Self::ActorType>;
}

impl<S, E, A, R, F, W> SpecifiesChild<W> for S
where
    S: StartableWith<W, Output = (Child<E, A>, R), Fut = F>,
    R: Restartable<Exit = E, ActorType = A>,
    F: Future<Output = Result<(Child<E, A>, R), StartError>> + Send,
    A: ActorType,
    E: Send + 'static,
{
    type Exit = E;
    type ActorType = A;
    type Restarter = R;
}

//------------------------------------------------------------------------------------------------
//  StartableWith
//------------------------------------------------------------------------------------------------

/// Anything that implements [Startable] can be started using [Startable::start]. This returns a
/// `Future` with as its output `Result<Self::Output, StartError>`.
pub trait StartableWith<W>: Send + 'static {
    /// The value returned upon successfully starting a process.
    type Output;

    /// The future returned when starting this process.
    type Fut: Future<Output = Result<Self::Output, StartError>> + Send;

    /// Starts the actor. If successful this returns `Result<Self::Output, StartError>`, otherwise
    /// this returns [Ok(StartError)](StartError).
    fn start_with(with: W) -> Self::Fut;
}

/// A type-alias for a `Pin<Box<dyn Future<_>>>` to use for [Startable::StartFut].
pub type BoxStartFut<O> = BoxFuture<'static, Result<O, StartError>>;

//------------------------------------------------------------------------------------------------
//  Startable
//------------------------------------------------------------------------------------------------

pub trait Startable: StartableWith<Self> + Sized {
    fn start(self) -> Self::Fut {
        <Self as StartableWith<Self>>::start_with(self)
    }
}

fn test<S: Startable>() {
    let x: S::Fut = todo!();
}

impl<T> Startable for T where T: StartableWith<Self> + Sized {}

//------------------------------------------------------------------------------------------------
//  Restartable
//------------------------------------------------------------------------------------------------

/// Anything that implements [Restartable] can be restarted using [Restartable::should_restart],
/// and then calling [Startable::start] on the result of that.
pub trait Restartable: Send + 'static {
    /// The value this actor exits with.
    type Exit: Send + 'static;

    /// The [ActorType] of this process.
    type ActorType: ActorType;

    /// The value returned upon successfully starting the process.
    type Starter: SpecifiesChild<Exit = Self::Exit, ActorType = Self::ActorType, Restarter = Self>;

    /// The future returned when deciding whether to restart a process.
    type Fut: Future<Output = Result<Option<Self::Starter>, StartError>> + Send;

    /// Decides whether the process should restart.
    ///
    /// If this returns
    ///
    /// TODO: Pipe in the Child here to give more information about the process.
    fn should_restart(self, exit: Result<Self::Exit, ExitError>) -> Self::Fut;
}

/// A type-alias for a `Pin<Box<dyn Future<_>>>` to use for [Restartable::ShouldRestartFut].
pub type BoxRestartFut<S> = BoxFuture<'static, Result<Option<S>, StartError>>;

//------------------------------------------------------------------------------------------------
//  SpawnableWith
//------------------------------------------------------------------------------------------------

pub trait SpawnableWith<W>: Send + 'static {
    type Output: Send;

    fn spawn_with(with: W) -> Result<Self::Output, StartError>;
}

impl<T, W> StartableWith<W> for T
where
    T: SpawnableWith<W>,
{
    type Output = <T as SpawnableWith<W>>::Output;
    type Fut = Ready<Result<Self::Output, StartError>>;

    fn start_with(with: W) -> Self::Fut {
        ready(Self::spawn_with(with))
    }
}

//------------------------------------------------------------------------------------------------
//  Spawnable
//------------------------------------------------------------------------------------------------

pub trait Spawnable: SpawnableWith<Self> + Sized {
    fn spawn(self) -> Result<Self::Output, StartError> {
        Self::spawn_with(self)
    }
}

impl<T> Spawnable for T where T: SpawnableWith<Self> + Sized {}

//------------------------------------------------------------------------------------------------
//  StartError
//------------------------------------------------------------------------------------------------

/// A generic error returned when a process failed to start.
#[derive(Debug)]
pub struct StartError(pub Box<dyn Error + Send>);

impl StartError {
    pub fn new<E: Error + Send + 'static>(error: E) -> Self {
        Self(Box::new(error))
        // Stream
    }
}

impl<E: Error + Send + 'static> From<E> for StartError {
    fn from(value: E) -> Self {
        Self::new(value)
    }
}

//------------------------------------------------------------------------------------------------
//  RawStarter
//------------------------------------------------------------------------------------------------

pub struct RawStarter<Fut, O>(Fut)
where
    Fut: Future<Output = Result<O, StartError>> + Send + 'static,
    O: Send + 'static;

impl<Fut, O> RawStarter<Fut, O>
where
    Fut: Future<Output = Result<O, StartError>> + Send + 'static,
    O: Send + 'static,
{
    pub fn new(fut: Fut) -> Self {
        Self(fut)
    }
}

impl<Fut, O> StartableWith<Self> for RawStarter<Fut, O>
where
    Fut: Future<Output = Result<O, StartError>> + Send + 'static,
    O: Send + 'static,
{
    // type With = Self;
    type Output = O;
    type Fut = Fut;

    fn start_with(with: Self) -> Self::Fut {
        with.0
    }
}

pub struct RawRestarter<Fun, Fut, E, S, A>(Fun, PhantomData<Fut>)
where
    Fun: FnOnce(Result<E, ExitError>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Option<S>, StartError>> + Send + 'static,
    S: SpecifiesChild<Exit = E, ActorType = A, Restarter = Self>,
    A: ActorType + 'static,
    E: Send + 'static;

impl<Fun, Fut, E, S, A> RawRestarter<Fun, Fut, E, S, A>
where
    Fun: FnOnce(Result<E, ExitError>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Option<S>, StartError>> + Send + 'static,
    S: SpecifiesChild<Exit = E, ActorType = A, Restarter = Self>,
    A: ActorType + 'static,
    E: Send + 'static,
{
    pub fn new(function: Fun) -> Self {
        Self(function, PhantomData)
    }
}

impl<Fun, Fut, E, S, A> Restartable for RawRestarter<Fun, Fut, E, S, A>
where
    Fun: FnOnce(Result<E, ExitError>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Option<S>, StartError>> + Send + 'static,
    S: SpecifiesChild<Exit = E, ActorType = A, Restarter = Self>,
    A: ActorType + 'static,
    E: Send + 'static,
{
    type Exit = E;
    type ActorType = A;
    type Starter = S;
    type Fut = Fut;

    fn should_restart(self, exit: Result<Self::Exit, ExitError>) -> Self::Fut {
        self.0(exit)
    }
}
