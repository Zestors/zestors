#[allow(unused)]
use crate::all::*;
use tokio::task::JoinHandle;

/// The parameter `C` in a [`Child<_, _, C>`] that specifies what kind of child it is:
/// - [`SingleProcess`] -> The actor consists of a single process: [`Child<_, _>`].
/// - [`MultiProcess`] -> The actor consists of multiple processes: [`ChildPool<_, _>`].
pub trait ChildType {
    type JoinHandles<E: Send + 'static>: Send + 'static;
    fn abort<E: Send + 'static>(handles: &Self::JoinHandles<E>);
    fn is_finished<E: Send + 'static>(handles: &Self::JoinHandles<E>) -> bool;
}

/// The default [`ChildType`].
#[derive(Debug)]
pub struct SingleProcess;

impl ChildType for SingleProcess {
    type JoinHandles<E: Send + 'static> = JoinHandle<E>;

    fn abort<E: Send + 'static>(handles: &Self::JoinHandles<E>) {
        handles.abort()
    }

    fn is_finished<E: Send + 'static>(handles: &Self::JoinHandles<E>) -> bool {
        handles.is_finished()
    }
}

/// The pooled [`ChildType`].
#[derive(Debug)]
pub struct MultiProcess;

impl ChildType for MultiProcess {
    type JoinHandles<E: Send + 'static> = Vec<JoinHandle<E>>;

    fn abort<E: Send + 'static>(handles: &Self::JoinHandles<E>) {
        for handle in handles {
            handle.abort()
        }
    }

    fn is_finished<E: Send + 'static>(handles: &Self::JoinHandles<E>) -> bool {
        handles.iter().all(|handle| handle.is_finished())
    }
}
