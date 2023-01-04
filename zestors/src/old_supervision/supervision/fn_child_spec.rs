use crate::{self as zestors, *};
use async_trait::async_trait;
use futures::Future;
use std::{mem::swap};

pub struct SupervisableChild<P, I, R, F1, F2>
where
    R: Send + 'static,
{
    child: Child<R>,
    restart_fn: fn(Result<R, ExitError>) -> Option<F2>,
    spawn_fn: fn(Inbox<P>, I) -> F1,
    config: Config,
    state: ChildState<F2>,
}

#[async_trait]
impl<P, I, R, F1, F2> Supervisable for SupervisableChild<P, I, R, F1, F2>
where
    F1: Future<Output = R> + Send + 'static,
    F2: Future<Output = Result<I, RestartError>> + Send + 'static,
    P: Protocol,
    I: Send + 'static,
    R: Send + 'static,
{
    async fn supervise(&mut self) -> bool {
        match self.state {
            ChildState::Alive => { 
                let exit = (&mut self.child).await; 
                match (self.restart_fn)(exit) {
                    Some(restart_fut) => {
                        self.state = ChildState::Restarting(Box::pin(restart_fut));
                        true
                    },
                    None => {
                        self.state = ChildState::Finished;
                        false
                    }
                }
            }
            ChildState::Exited | ChildState::Finished | ChildState::Restarting(_) => {
                panic!("Child is not alive!")
            }
        }
    }

    async fn restart(&mut self) -> Result<(), RestartError> {
        let state = &mut self.state;
        let ChildState::Restarting(restart_fut) = state else { 
            panic!("Didn't want to restart!")
        };

        match restart_fut.await {
            Ok(init) => {
                let spawn_fn = self.spawn_fn;
                let (child, _address) =
                    spawn_process(Config::default(), move |inbox: Inbox<P>| async move {
                        spawn_fn(inbox, init).await
                    });
                let mut child = child.into_dyn();
                swap(&mut self.child, &mut child);
                self.state = ChildState::Alive;
                Ok(())
            }
            Err(e) => {
                self.state = ChildState::Finished;
                Err(e)
            }
        }
    }

    fn abort(&mut self) -> bool {
        self.child.abort()
    }

    fn halt(&self) {
        self.child.halt()
    }

    fn halt_some(&self, n: u32) {
        self.child.halt_some(n)
    }
}

impl<P, I, R, F1, F2> SupervisableChild<P, I, R, F1, F2>
where
    F1: Future<Output = R> + Send + 'static,
    F2: Future<Output = Result<I, RestartError>> + Send + 'static,
    P: Protocol,
    I: Send + 'static,
    R: Send + 'static,
{
    /// # Panics
    /// - If the child is finished.
    /// - If the child is not attached.
    pub fn new(
        child: Child<R>,
        restart_fn: fn(Result<R, ExitError>) -> Option<F2>,
        spawn_fn: fn(Inbox<P>, I) -> F1,
    ) -> Self {
        let config = Config { 
            link: child.link().clone(), 
            capacity: child.capacity().clone() 
        };

        assert!(config.link.is_attached(), "Can't supervise a child that is not attached");
        assert!(!child.is_finished(), "Can't supervise a child that is already finished");

        Self {
            child,
            restart_fn,
            spawn_fn,
            state: ChildState::Alive,
            config
        }
    }
}