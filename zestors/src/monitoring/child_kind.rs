#[allow(unused)]
use crate::*;
use tokio::task::JoinHandle;

pub trait ChildKind {
    type JoinHandles<E: Send + 'static>: Send + 'static;
    fn abort<E: Send + 'static>(handles: &Self::JoinHandles<E>);
    fn is_finished<E: Send + 'static>(handles: &Self::JoinHandles<E>) -> bool;
}

/// A [ChildKind] which indicates that the [Child] concerns a single process. This is also the
/// default parameter.
#[derive(Debug)]
pub struct Single;

impl ChildKind for Single {
    type JoinHandles<E: Send + 'static> = JoinHandle<E>;

    fn abort<E: Send + 'static>(handles: &Self::JoinHandles<E>) {
        handles.abort()
    }

    fn is_finished<E: Send + 'static>(handles: &Self::JoinHandles<E>) -> bool {
        handles.is_finished()
    }
}

/// A [ChildKind] which indicates that the [Child] concerns a group of processes.
#[derive(Debug)]
pub struct Group;

impl ChildKind for Group {
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
