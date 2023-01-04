use crate::*;
use futures::{Future, FutureExt, Stream};
use std::{
    error::Error,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use tiny_actor::{ExitError, Link};

//------------------------------------------------------------------------------------------------
//  Supervisor
//------------------------------------------------------------------------------------------------

pub struct SupervisorBuilder {
    children: Vec<Box<dyn DynamicallySpecifiesChild + Send>>,
}

impl SupervisorBuilder {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    pub fn add_spec<S: SpecifiesChild + Send + 'static>(self, spec: S) -> Self {
        self.add_boxed_spec(Box::new(spec))
    }

    pub fn add_boxed_spec(mut self, spec: Box<dyn DynamicallySpecifiesChild + Send>) -> Self {
        self.children.push(spec);
        self
    }

    pub async fn start(self) -> Result<(), StartError> {
        let mut supervised_children = Vec::new();

        for child in self.children {
            let supervised_child = child.start(Link::default()).await?;
            supervised_children.push(supervised_child);
        }

        Ok(())
    }
}

impl PrivSupervisable for () {
    type SuperviseFut = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

    fn supervise(&mut self) -> Self::SuperviseFut {
        PrivDynamicallySupervisable::supervise(self)
    }

    type ToRestartFut = Pin<Box<dyn Future<Output = Result<bool, StartError>> + Send + 'static>>;

    fn to_restart(&mut self) -> Self::ToRestartFut {
        PrivDynamicallySupervisable::to_restart(self)
    }

    type RestartFut = Pin<Box<dyn Future<Output = Result<(), StartError>> + Send + 'static>>;

    fn restart(&mut self) -> Self::RestartFut {
        PrivDynamicallySupervisable::restart(self)
    }
}

async fn test() {
    let supervisor = SupervisorBuilder::new()
        .add_spec(RawChildSpec::new(async move { Ok(()) }))
        .start()
        .await;
}

//------------------------------------------------------------------------------------------------
//  SpecifiesChild
//------------------------------------------------------------------------------------------------

pub trait SpecifiesChild {
    type Supervisable: PrivSupervisable + Send + 'static;
    type Future: Future<Output = Result<Self::Supervisable, StartError>> + Send + 'static;

    fn start(self) -> Self::Future;

    // fn poll_start
}

struct SpawnFnChildSpec<Fun, Fut, E, P, Sup>
where
    Fun: Fn(Inbox<P>) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send + 'static,
    E: Send + 'static,
    P: Protocol,
    Sup: PrivSupervisable + Send + 'static,
{
    fun: Fun,
    config: Config,
    phantom: PhantomData<(Fut, E, P, Sup)>,
}

impl<Fun, Fut, E, P, Sup> SpecifiesChild for SpawnFnChildSpec<Fun, Fut, E, P, Sup>
where
    Fun: Fn(Inbox<P>) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send + 'static,
    E: Send + 'static,
    P: Protocol,
    Sup: PrivSupervisable + Send + 'static,
{
    type Supervisable = Sup;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Supervisable, StartError>> + Send + 'static>>;

    fn start(self) -> Self::Future {
        Box::pin(async move {
            let (child, address) = spawn_process(self.config, move |inbox| async move {
                let fut = (self.fun)(inbox);
                (fut.await, self.fun)
            });

            todo!()
        })
    }
}

struct RawChildSpec<Fut, Sup>
where
    Fut: Future<Output = Result<Sup, StartError>> + Send + 'static,
    Sup: PrivSupervisable + Send + 'static,
{
    fun: Fut,
    phantom: PhantomData<(Fut, Sup)>,
}

impl<Fut, Sup> RawChildSpec<Fut, Sup>
where
    Fut: Future<Output = Result<Sup, StartError>> + Send + 'static,
    Sup: PrivSupervisable + Send + 'static,
{
    pub fn new(fun: Fut) -> Self {
        Self {
            fun,
            phantom: PhantomData,
        }
    }
}

impl<Fut, Sup> SpecifiesChild for RawChildSpec<Fut, Sup>
where
    Fut: Future<Output = Result<Sup, StartError>> + Send + 'static,
    Sup: PrivSupervisable + Send + 'static,
{
    type Supervisable = Sup;
    type Future = Fut;

    fn start(self) -> Self::Future {
        self.fun
    }
}

pub trait DynamicallySpecifiesChild: Send + 'static {
    fn start(
        self: Box<Self>,
        link: Link,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Box<dyn PrivDynamicallySupervisable + Send>, StartError>>
                + Send
                + 'static,
        >,
    >;
}

impl<T> DynamicallySpecifiesChild for T
where
    T: SpecifiesChild + Send + 'static,
    T::Supervisable: Send,
{
    fn start(
        self: Box<Self>,
        link: Link,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Box<dyn PrivDynamicallySupervisable + Send>, StartError>>
                + Send
                + 'static,
        >,
    > {
        Box::pin(async move {
            match <T as SpecifiesChild>::start(*self).await {
                Ok(child) => {
                    let child: Box<dyn PrivDynamicallySupervisable + Send> = Box::new(child);
                    Ok(child)
                }
                Err(e) => Err(e),
            }
        })
    }
}

//------------------------------------------------------------------------------------------------
//  Supervisable
//------------------------------------------------------------------------------------------------

pub struct SupervisableChild<E, P, S>
where
    E: Send + 'static,
    P: Protocol,
    S: Supervisable,
{
    child: Child<E, P>,
    supervisable: S,
    // state: ChildState<E>,
}

pub trait Supervisable {
    /// After the process has exited, it will be polled whether it wants to be restarted.
    /// This should return `Ok(true)` if it wants to be restarted, and `Ok(false)` if the
    /// process is finished.
    ///
    /// If the process is not finished, but can't restart because of a problem, then an
    /// `Err(StartError)` may be returned. This will cause the supervisor itself to exit
    /// and restart.
    ///
    /// # Panics
    /// This function is allowed to panic if:
    /// - The process is still alive.
    /// - `poll_start` has not been called before this function.
    fn poll_to_restart(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<bool, StartError>>;

    /// This is polled whenever the child needs to be started. This can either for the first
    /// start, or after `poll_restart` is called.
    ///
    /// # Panics
    /// This function is allowed to panic if:
    /// - The process is still alive.
    /// - `poll_restart` returned something other than `Ok(true)` before calling this.
    /// - `poll_restart` was not called before attempting a restart.
    fn poll_restart(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), StartError>>;
}

//------------------------------------------------------------------------------------------------
//  PrivSupervisable
//------------------------------------------------------------------------------------------------

// pub struct SupervisableChild<E, P, S>
// where
//     E: Send + 'static,
//     P: Protocol,
//     S: SpecifiesChild<Exit = E, Protocol = P>,
// {
//     child: Child<E, P>,
//     startable: S,
//     state: ChildState<E>,
// }

pub trait PrivSupervisable {
    type SuperviseFut: Future<Output = ()> + Send + 'static;
    fn supervise(&mut self) -> Self::SuperviseFut;

    type ToRestartFut: Future<Output = Result<bool, StartError>> + Send + 'static;
    fn to_restart(&mut self) -> Self::ToRestartFut;

    type RestartFut: Future<Output = Result<(), StartError>> + Send + 'static;
    fn restart(&mut self) -> Self::RestartFut;
}

pub trait PrivDynamicallySupervisable: Send + 'static {
    fn supervise(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
    fn to_restart(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<bool, StartError>> + Send + 'static>>;
    fn restart(&mut self)
        -> Pin<Box<dyn Future<Output = Result<(), StartError>> + Send + 'static>>;
}

// impl Supervisable for Box<dyn DynamicallySupervisable + Send> {
//     type SuperviseFut = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

//     fn supervise(&mut self) -> Self::SuperviseFut {
//         DynamicallySupervisable::supervise(self)
//     }

//     type ToRestartFut = Pin<Box<dyn Future<Output = Result<bool, StartError>> + Send + 'static>>;

//     fn to_restart(&mut self) -> Self::ToRestartFut {
//         DynamicallySupervisable::to_restart(self)
//     }

//     type RestartFut = Pin<Box<dyn Future<Output = Result<(), StartError>> + Send + 'static>>;

//     fn restart(&mut self) -> Self::RestartFut {
//         DynamicallySupervisable::restart(self)
//     }
// }

impl<T: PrivSupervisable + Send + 'static> PrivDynamicallySupervisable for T {
    fn supervise(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(<T as PrivSupervisable>::supervise(self))
    }

    fn to_restart(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<bool, StartError>> + Send + 'static>> {
        Box::pin(<T as PrivSupervisable>::to_restart(self))
    }

    fn restart(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), StartError>> + Send + 'static>> {
        Box::pin(<T as PrivSupervisable>::restart(self))
    }
}

//------------------------------------------------------------------------------------------------
//  StartError
//------------------------------------------------------------------------------------------------

/// An error returned when starting of a process has failed.
///
/// This is a simple wrapper around a `Box<dyn Error + Send>`, and can therefore be used for any
/// type that implements `Error + Send + 'static`.
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
