use futures::future::BoxFuture;

use crate::*;

//------------------------------------------------------------------------------------------------
//  DynamicChildSpec
//------------------------------------------------------------------------------------------------

pub struct DynamicChildSpec(Box<dyn PrivDynamicallySpecifiesChild>);

impl DynamicChildSpec {
    pub fn new<S: SpecifiesChild>(spec: S) -> Self {
        let boxed: BoxFuture<_> = Box::pin(async move {
            spec.start()
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
    T: Startable<
        Output = DynamicSupervisee,
        Fut = BoxFuture<'static, Result<DynamicSupervisee, StartError>>,
    >,
{
    fn start(self: Box<Self>) -> BoxFuture<'static, Result<DynamicSupervisee, StartError>> {
        (*self).start()
    }
}

//------------------------------------------------------------------------------------------------
//  SimpleChildSpec
//------------------------------------------------------------------------------------------------

pub struct SimpleChildSpec<P, E>
where
    P: Protocol,
    E: Send + 'static,
{
    spawn_fn: fn(Inbox<P>) -> E,
    config: Config,
    restart_strategy: RestartStrategy,
}

pub enum RestartStrategy {
    Always,
    NormalExitOnly,
    Never,
}

impl<P: Protocol, E: Send + 'static> SimpleChildSpec<P, E> {
    pub fn new(
        spawn_fn: fn(Inbox<P>) -> E,
        config: Config,
        restart_strategy: RestartStrategy,
    ) -> Self {
        Self {
            spawn_fn,
            config,
            restart_strategy,
        }
    }
}

impl<P: Protocol, E: Send + 'static> StartableWith<Self> for SimpleChildSpec<P, E> {
    type Output = (Child<E, P>, Self);
    type Fut = BoxStartFut<Self::Output>;

    fn start_with(with: Self) -> Self::Fut {
        Box::pin(async move {
            let spawn_fun = with.spawn_fn;
            let config = with.config.clone();
            let (child, _address) =
                spawn_process(config, move |inbox| async move { spawn_fun(inbox) });

            Ok((child, with))
        })
    }
}

impl<P: Protocol, E: Send + 'static> Restartable for SimpleChildSpec<P, E> {
    type Exit = E;
    type ActorType = P;
    type Starter = Self;
    type Fut = BoxRestartFut<Self::Starter>;

    fn should_restart(self, exit: Result<Self::Exit, ExitError>) -> Self::Fut {
        Box::pin(async move {
            match (exit, &self.restart_strategy) {
                (_, RestartStrategy::Always) | (Ok(_), RestartStrategy::NormalExitOnly) => {
                    Ok(Some(self))
                }
                (Err(_), RestartStrategy::NormalExitOnly) | (_, RestartStrategy::Never) => Ok(None),
            }
        })
    }
}
