use std::future::{ready, Ready};

use futures::{future::BoxFuture, Future};

use crate::*;

//------------------------------------------------------------------------------------------------
//  DynamicChildSpec
//------------------------------------------------------------------------------------------------

pub struct DynamicChildSpec(Box<dyn PrivDynamicallySpecifiesChild>);

impl DynamicChildSpec {
    pub fn new<S: SpecifiesChild>(spec: S) -> Self {
        let boxed: BoxFuture<_> = Box::pin(async move {
            S::start(spec)
                .await
                .map(|(child, restarter)| Supervisee::new(child, restarter).into_dyn())
        });
        Self(Box::new(RawStarter::new(boxed)))
    }

    pub fn start(self) -> BoxFuture<'static, Result<DynamicSupervisee, StartError>> {
        self.0.start()
    }
}

impl<S: SpecifiesChild> From<S> for DynamicChildSpec {
    fn from(value: S) -> Self {
        Self::new(value)
    }
}

trait PrivDynamicallySpecifiesChild: Send {
    fn start(self: Box<Self>) -> BoxFuture<'static, Result<DynamicSupervisee, StartError>>;
}

impl<T> PrivDynamicallySpecifiesChild for T
where
    T: Startable<Ok = DynamicSupervisee, Fut = BoxStartFut<DynamicSupervisee>>,
{
    fn start(self: Box<Self>) -> BoxStartFut<DynamicSupervisee> {
        (*self).start()
    }
}

//------------------------------------------------------------------------------------------------
//  SimpleChildSpec
//------------------------------------------------------------------------------------------------

pub struct ChildSpecWithout<P, E, I, SFut, RFut>
where
    P: Protocol,
    E: Send + 'static,
    SFut: Future<Output = E> + Send + 'static,
    RFut: Future<Output = Result<Option<I>, StartError>> + Send + 'static,
{
    spawn_fn: fn(Inbox<P>, I) -> SFut,
    restart_fn: fn(Result<E, ExitError>) -> RFut,
    config: Config,
}

impl<P, E, I, SFut, RFut> ChildSpecWithout<P, E, I, SFut, RFut>
where
    P: Protocol,
    E: Send + 'static,
    I: Send + 'static,
    SFut: Future<Output = E> + Send + 'static,
    RFut: Future<Output = Result<Option<I>, StartError>> + Send + 'static,
{
    pub fn new(
        spawn_fn: fn(Inbox<P>, I) -> SFut,
        restart_fn: fn(Result<E, ExitError>) -> RFut,
        config: Config,
    ) -> Self {
        Self {
            spawn_fn,
            config,
            restart_fn,
        }
    }
}

// impl<P, E, I, SFut, RFut> SpawnableFrom<(Self, I)> for ChildSpecWithout<P, E, I, SFut, RFut>
// where
//     P: Protocol,
//     E: Send + 'static,
//     I: Send + 'static,
//     SFut: Future<Output = E> + Send + 'static,
//     RFut: Future<Output = Result<Option<I>, StartError>> + Send + 'static,
// {
//     type Output = (Child<E, P>, ChildSpecWith<P, E, I, SFut, RFut>);

//     fn spawn_from((from, init): (Self, I)) -> Self::Output {
//         let spawn_fun = from.spawn_fn;
//         let config = from.config.clone();
//         let (child, _address) =
//             spawn_process(
//                 config,
//                 move |inbox| async move { spawn_fun(inbox, init).await },
//             );
//         (
//             child,
//             ChildSpecWith {
//                 spawn_fn: from.spawn_fn,
//                 restart_fn: from.restart_fn,
//                 config: from.config,
//                 init: None,
//             },
//         )
//     }
// }

// impl<P, E, I, SFut, RFut> StartableFrom<(Self, I)> for ChildSpecWithout<P, E, I, SFut, RFut>
// where
//     P: Protocol,
//     E: Send + 'static,
//     I: Send + 'static,
//     SFut: Future<Output = E> + Send + 'static,
//     RFut: Future<Output = Result<Option<I>, StartError>> + Send + 'static,
// {
//     type Ok = (Child<E, P>, ChildSpecWith<P, E, I, SFut, RFut>);
//     type Fut = Ready<Result<Self::Ok, StartError>>;

//     fn start_from(from: (Self, I)) -> Self::Fut {
//         ready(Ok(Self::spawn_from(from)))
//     }
// }

//------------------------------------------------------------------------------------------------
//
//------------------------------------------------------------------------------------------------

pub struct ChildSpecWith<P, E, I, SFut, RFut>
where
    P: Protocol,
    E: Send + 'static,
    SFut: Future<Output = E> + Send + 'static,
    RFut: Future<Output = Result<Option<I>, StartError>> + Send + 'static,
{
    spawn_fn: fn(Inbox<P>, I) -> SFut,
    restart_fn: fn(Result<E, ExitError>) -> RFut,
    config: Config,
    init: Option<I>,
}

impl<P, E, I, SFut, RFut> ChildSpecWith<P, E, I, SFut, RFut>
where
    P: Protocol,
    E: Send + 'static,
    I: Send + 'static,
    SFut: Future<Output = E> + Send + 'static,
    RFut: Future<Output = Result<Option<I>, StartError>> + Send + 'static,
{
    pub fn new(
        spawn_fn: fn(Inbox<P>, I) -> SFut,
        restart_fn: fn(Result<E, ExitError>) -> RFut,
        config: Config,
        init: I,
    ) -> Self {
        Self {
            spawn_fn,
            config,
            init: Some(init),
            restart_fn,
        }
    }
}

impl<P, E, I, SFut, RFut> Spawnable for ChildSpecWith<P, E, I, SFut, RFut>
where
    P: Protocol,
    E: Send + 'static,
    I: Send + 'static,
    SFut: Future<Output = E> + Send + 'static,
    RFut: Future<Output = Result<Option<I>, StartError>> + Send + 'static,
{
    fn spawn(mut self) -> (Child<E, P>, Self::Ref) {
        let spawn_fun = self.spawn_fn;
        let config = self.config.clone();
        let data = self.init.take().unwrap();
        let (child, _address) =
            spawn_process(
                config,
                move |inbox| async move { spawn_fun(inbox, data).await },
            );
        (child, self)
    }

    type Exit = E;
    type ActorType = P;
    type Ref = Self;
}

impl<P, E, D, SFut, RFut> Restartable for ChildSpecWith<P, E, D, SFut, RFut>
where
    P: Protocol,
    E: Send + 'static,
    D: Send + 'static,
    SFut: Future<Output = E> + Send + 'static,
    RFut: Future<Output = Result<Option<D>, StartError>> + Send + 'static,
{
    type Exit = E;
    type ActorType = P;
    type Starter = Self;
    type Fut = BoxRestartFut<Self::Starter>;

    fn should_restart(mut self, exit: Result<Self::Exit, ExitError>) -> Self::Fut {
        Box::pin(async move {
            self.init = (self.restart_fn)(exit).await?;
            match &self.init {
                Some(_) => Ok(Some(self)),
                None => Ok(None),
            }
        })
    }
}

pub enum RestartStrategy {
    Always,
    NormalExitOnly,
    Never,
}

impl RestartStrategy {
    fn restart<E>(&self, exit: &Result<E, ExitError>) -> bool {
        match (exit, self) {
            (_, RestartStrategy::Always) | (Ok(_), RestartStrategy::NormalExitOnly) => true,
            (Err(_), RestartStrategy::NormalExitOnly) | (_, RestartStrategy::Never) => false,
        }
    }
}
