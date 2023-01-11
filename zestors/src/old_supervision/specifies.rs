use crate::*;
use futures::{future::BoxFuture, Future};
use std::{
    error::Error,
    future::{ready, Ready},
    marker::PhantomData,
};

//------------------------------------------------------------------------------------------------
//  Spawnable
//------------------------------------------------------------------------------------------------
//  SpawnableFrom
//-------------------------------------------------

/// Trait to allow a type to be spawned with value `W`. If `W = Self`, then [`Spawnable`] is
/// automatically implemented for your type as well.
///
/// `SpawnableFrom` is just an infallible and synchronous variant of [`StartableFrom`], so
/// implementing `SpawnableFrom<W>` automatically implements `StartableFrom<W>` as well. If your
/// type can be started synchronously, it is therefore better to implement `SpawnableFrom` instead
/// of `StartableFrom`.
pub trait Spawnable: Send + 'static {
    type Exit: Send + 'static;
    type ActorType: ActorType + 'static;
    type Ref: Send + 'static;

    fn spawn(self) -> (Child<Self::Exit, Self::ActorType>, Self::Ref);
}

//------------------------------------------------------------------------------------------------
//  Startable
//------------------------------------------------------------------------------------------------
//  StartableFrom
//-------------------------------------------------

/// Anything that implements [`Startable`] can be started using [`Startable::start`]. This returns a
/// [`Future`] with as its output a `Result<Self::Ok, StartError>`.
pub trait Startable: Send + 'static {
    type Exit: Send + 'static;
    type ActorType: ActorType + 'static;
    type Ref: Send + 'static;
    /// The future returned when starting this process.
    type Fut: Future<Output = Result<(Child<Self::Exit, Self::ActorType>, Self::Ref), StartError>>
        + Send;

    /// Starts the actor. If successful this returns `Result<Self::Ok, StartError>`, otherwise
    /// this returns [Ok(StartError)](StartError).
    fn start(self) -> Self::Fut;
}

impl<T> Startable for T
where
    T: Spawnable,
{
    type Exit = T::Exit;
    type ActorType = T::ActorType;
    type Ref = T::Ref;
    type Fut = Ready<Result<(Child<T::Exit, T::ActorType>, T::Ref), StartError>>;

    fn start(self) -> Self::Fut {
        ready(Ok(self.spawn()))
    }
}

/// A type-alias for a `Pin<Box<dyn Future<_>>>` to use for [Startable::StartFut].
pub type BoxStartFut<Output> = BoxFuture<'static, Result<Output, StartError>>;

//-------------------------------------------------
//  StartError
//-------------------------------------------------

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
//  SpecifiesChild
//------------------------------------------------------------------------------------------------
//  SpecifiesChildFrom
//-------------------------------------------------

/// An auto-trait implemented for type that implements [`StartableFrom`] where
/// `StartableFrom::Ok = (Child<_, _>, R)` and [`R: Restartable`](Restartable).
pub trait SpecifiesChild: Startable + Sized
where
    Self::Ref: Restartable<Exit = Self::Exit, ActorType = Self::ActorType>,
{
}

impl<S, E, A, R, Fut> SpecifiesChild for S
where
    S: Startable<Exit = E, Fut = Fut, ActorType = A, Ref = R>,
    R: Restartable<Exit = Self::Exit, ActorType = Self::ActorType>,
{
}

//------------------------------------------------------------------------------------------------
//  Restartable
//------------------------------------------------------------------------------------------------
//  Restartable
//-------------------------------------------------

/// Anything that implements [Restartable] can be restarted using [Restartable::should_restart],
/// and then calling [Startable::start] on the result of that.
pub trait Restartable: Send + 'static {
    /// The value this actor exits with.
    type Exit: Send + 'static;

    /// The [ActorType] of this process.
    type ActorType: ActorType;

    /// The value returned upon successfully starting the process.
    type Starter: SpecifiesChild<Exit = Self::Exit, ActorType = Self::ActorType, Ref = Self>;

    /// The future returned when deciding whether to restart a process.
    type Fut: Future<Output = Result<Option<Self::Starter>, StartError>> + Send;

    /// Decides whether the process should restart.
    fn should_restart(self, exit: Result<Self::Exit, ExitError>) -> Self::Fut;
}

//-------------------------------------------------
//  BoxRestartFut
//-------------------------------------------------

/// A type-alias for a `Pin<Box<dyn Future<_>>>` to use for [Restartable::ShouldRestartFut].
pub type BoxRestartFut<Starter> = BoxFuture<'static, Result<Option<Starter>, StartError>>;

//------------------------------------------------------------------------------------------------
//  Raw
//------------------------------------------------------------------------------------------------
//  RawStarter
//-------------------------------------------------

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

impl<Fut, O> Startable for RawStarter<Fut, O>
where
    Fut: Future<Output = Result<O, StartError>> + Send + 'static,
    O: Send + 'static,
{
    // type With = Self;
    type Ok = O;
    type Fut = Fut;

    fn start(self) -> Self::Fut {
        self.0
    }
}

//-------------------------------------------------
//  RawRestarter
//-------------------------------------------------

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

//------------------------------------------------------------------------------------------------
//  Ext
//------------------------------------------------------------------------------------------------

pub trait StartableExt: Startable + Sized {
    fn map_after_start<Fun, Fut, O>(
        self,
        fun: Fun,
    ) -> RawStarter<BoxFuture<'static, Result<O, StartError>>, O>
    where
        Fun: FnOnce(Result<Self::Ok, StartError>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<O, StartError>> + Send + 'static,
        O: Send + 'static,
    {
        RawStarter::new(Box::pin(async move {
            let output = self.start().await;
            fun(output).await
        }))
    }

    fn map_before_start<Fun, Fut, Out>(
        self,
        fun: Fun,
    ) -> RawStarter<BoxFuture<'static, Result<Out::Ok, StartError>>, Out::Ok>
    where
        Fun: FnOnce(Self) -> Fut + Send + 'static,
        Fut: Future<Output = Out> + Send + 'static,
        Out: Startable,
    {
        RawStarter::new(Box::pin(async move {
            let starter = fun(self).await;
            starter.start().await
        }))
    }
}

impl<T> StartableExt for T where T: Startable {}

pub trait SpecifiesChildExt: SpecifiesChild {
    fn into_dyn(self) -> DynamicChildSpec {
        DynamicChildSpec::new(self)
    }

    fn start_supervise(
        self,
    ) -> BoxFuture<'static, Result<Supervisee<Self::Restarter>, StartError>> {
        Box::pin(async move {
            let (child, restartable) = self.start().await?;
            Ok(Supervisee::new(child, restartable))
        })
    }

    fn start_supervise_dyn(self) -> BoxFuture<'static, Result<DynamicSupervisee, StartError>> {
        Box::pin(async move {
            let (child, restartable) = self.start().await?;
            Ok(Supervisee::new(child, restartable).into_dyn())
        })
    }
}

impl<T> SpecifiesChildExt for T where T: SpecifiesChild {}
