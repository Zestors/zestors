#[allow(unused)]
use crate::*;
use tokio::task::JoinHandle;

pub trait ChildType {
    type JoinHandles<E: Send + 'static>: Send + 'static;
    fn abort<E: Send + 'static>(handles: &Self::JoinHandles<E>);
    fn is_finished<E: Send + 'static>(handles: &Self::JoinHandles<E>) -> bool;
}

/// A [ChildKind] which indicates that the [Child] concerns a single process. This is also the
/// default parameter.
#[derive(Debug)]
pub struct Single;

impl ChildType for Single {
    type JoinHandles<E: Send + 'static> = JoinHandle<E>;

    fn abort<E: Send + 'static>(handles: &Self::JoinHandles<E>) {
        handles.abort()
    }

    fn is_finished<E: Send + 'static>(handles: &Self::JoinHandles<E>) -> bool {
        handles.is_finished()
    }
}

/// A [ChildKind] which indicates that the [Child] concerns a pool of processes.
#[derive(Debug)]
pub struct Pool;

impl ChildType for Pool {
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
