use crate::{self as zestors, *};
use futures::Future;

/// The child-specification specifies how a child can be spawned and subsequently supervised.
///
/// A child-specification can be spawned with `spawn()`, which returns a Box<dyn Supervisable + Send>.
pub trait SpecifiesChild {
    // TODO: With RPITIT we can return `impl Supervisable + Send` instead.
    fn spawn(self) -> Box<dyn Supervisable + Send>;
}

pub struct ChildSpec<P, I, R, F1, F2> {
    pub run_fn: fn(Inbox<P>, I) -> F1,
    pub init: I,
    pub config: Config,
    pub restart_fn: fn(Result<R, ExitError>) -> Option<F2>,
}

impl<P, I, R, F1, F2> SpecifiesChild for ChildSpec<P, I, R, F1, F2>
where
    F1: Future<Output = R> + Send + 'static,
    F2: Future<Output = Result<I, RestartError>> + Send + 'static,
    P: Protocol,
    I: Send + 'static,
    R: Send + 'static,
{
    fn spawn(self) -> Box<dyn Supervisable + Send> {
        let (child, _address) = spawn_process(self.config, move |inbox| async move {
            (self.run_fn)(inbox, self.init).await
        });
        Box::new(SupervisableChild::new(
            child.into_dyn(),
            self.restart_fn,
            self.run_fn,
        ))
    }
}
