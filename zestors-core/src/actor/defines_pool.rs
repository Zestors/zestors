use tokio::task::JoinHandle;

pub trait DefinesPool {
    type JoinHandles<E: Send + 'static>: Send + 'static;
    fn abort<E: Send + 'static>(handles: &Self::JoinHandles<E>);
    fn is_finished<E: Send + 'static>(handles: &Self::JoinHandles<E>) -> bool;
}

#[derive(Debug)]
pub struct NoPool;

impl DefinesPool for NoPool {
    type JoinHandles<E: Send + 'static> = JoinHandle<E>;

    fn abort<E: Send + 'static>(handles: &Self::JoinHandles<E>) {
        handles.abort()
    }

    fn is_finished<E: Send + 'static>(handles: &Self::JoinHandles<E>) -> bool {
        handles.is_finished()
    }
}

#[derive(Debug)]
pub struct Pool;

impl DefinesPool for Pool {
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
