use crate::*;
use futures::{future::BoxFuture, pin_mut, Future};
use std::{error::Error, marker::PhantomData, pin::Pin};

// ------------------------------------------------------------------------------------------------
//  SpecifiesChild
// ------------------------------------------------------------------------------------------------

pub trait SpecifiesChild:
    Startable<Output = (Child<Self::Exit, Self::ActorType>, Self)> + Send + 'static
{
    type Exit: Send + 'static;
    type ActorType: ActorType;
}

impl<S, E, A, F> SpecifiesChild for S
where
    S: Startable<Output = (Child<E, A>, Self), StartFut = F>,
    F: Future<Output = Result<(Child<E, A>, Self), StartError>> + Send,
    A: ActorType,
    E: Send + 'static,
{
    type Exit = E;
    type ActorType = A;
}

//------------------------------------------------------------------------------------------------
//  Startable
//------------------------------------------------------------------------------------------------

/// Anything that implements [Startable] can be started using [Startable::start]. This returns a
/// `Future` with as its output `Result<Self::Output, StartError>`.
pub trait Startable: Send + 'static {
    /// The value returned upon successfully starting a process.
    type Output;

    /// The future returned when starting this process.
    type StartFut: Future<Output = Result<Self::Output, StartError>> + Send;

    /// Starts the actor. If successful this returns `Result<Self::Output, StartError>`, otherwise
    /// this returns [Ok(StartError)](StartError).
    fn start(self) -> Self::StartFut;
}

/// A type-alias for a `Pin<Box<dyn Future<_>>>` to use for [Startable::StartFut].
pub type BoxStartFut<O> = BoxFuture<'static, Result<O, StartError>>;

impl<T, O> Startable for T
where
    T: Future<Output = Result<O, StartError>> + Send + 'static,
{
    type Output = O;
    type StartFut = T;

    fn start(self) -> Self::StartFut {
        self
    }
}

//------------------------------------------------------------------------------------------------
//  Restartable
//------------------------------------------------------------------------------------------------

/// Anything that implements [Restartable] can be restarted using [Restartable::should_restart],
/// and then calling [Startable::start] on the result of that.
pub trait Restartable: Startable + Sized + Send + 'static {
    /// The value this actor exits with.
    type Exit: Send + 'static;

    /// The [ActorType] of this process.
    type ActorType: ActorType;

    /// The future returned when deciding whether to restart a process.
    type RestartFut: Future<Output = Result<Option<Self>, StartError>> + Send;

    /// Decides whether the process should restart.
    ///
    /// If this returns
    ///
    /// TODO: Pipe in the Child here to give more information about the process.
    fn should_restart(self, exit: Result<Self::Exit, ExitError>) -> Self::StartFut;
}

/// A type-alias for a `Pin<Box<dyn Future<_>>>` to use for [Restartable::ShouldRestartFut].
pub type BoxRestartFut<S> = BoxFuture<'static, Result<Option<S>, StartError>>;

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
