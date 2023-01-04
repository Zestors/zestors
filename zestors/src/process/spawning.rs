use crate::*;
use futures::Future;

pub fn spawn_process<P, E, Fun, Fut>(config: Config, fun: Fun) -> (Child<E, P>, Address<P>)
where
    Fun: FnOnce(Inbox<P>) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send + 'static,
    E: Send + 'static,
    P: Protocol,
{
    let (child, addr) = tiny_actor::spawn_process(config, |inbox| async move {
        fun(Inbox::from_inner(inbox)).await
    });
    (Child::from_inner(child), Address::from_inner(addr))
}

pub fn spawn_task<E, Fun, Fut>(config: Config, fun: Fun) -> Child<E>
where
    Fun: FnOnce(HaltNotifier) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send + 'static,
    E: Send + 'static,
{
    let child = tiny_actor::spawn_task(config, fun);
    todo!()
    // Child::from_inner(child)
}

pub fn spawn_one_process<P, E, Fun, Fut>(config: Config, fun: Fun) -> (ChildPool<E, P>, Address<P>)
where
    Fun: FnOnce(Inbox<P>) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send + 'static,
    E: Send + 'static,
    P: Protocol,
{
    let (child, addr) = tiny_actor::spawn_one_process(config, |inbox| async move {
        fun(Inbox::from_inner(inbox)).await
    });
    (ChildPool::from_inner(child), Address::from_inner(addr))
}

pub fn spawn_one_task<E, Fun, Fut>(config: Config, fun: Fun) -> ChildPool<E>
where
    Fun: FnOnce(HaltNotifier) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send + 'static,
    E: Send + 'static,
{
    todo!()
}

pub fn spawn_many_processes<P, E, I, Fun, Fut>(
    iter: impl IntoIterator<Item = I>,
    config: Config,
    fun: Fun,
) -> (ChildPool<E, P>, Address<P>)
where
    Fun: FnOnce(I, Inbox<P>) -> Fut + Send + 'static + Clone,
    Fut: Future<Output = E> + Send + 'static,
    E: Send + 'static,
    P: Protocol,
    I: Send + 'static,
{
    let (child, addr) = tiny_actor::spawn_many_processes(iter, config, |i, inbox| async move {
        fun(i, Inbox::from_inner(inbox)).await
    });
    (ChildPool::from_inner(child), Address::from_inner(addr))
}

pub fn spawn_many_tasks<E, I, Fun, Fut>(
    iter: impl IntoIterator<Item = I>,
    config: Config,
    fun: Fun,
) -> ChildPool<E>
where
    Fun: FnOnce(I, HaltNotifier) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = E> + Send + 'static,
    E: Send + 'static,
    I: Send + 'static,
{
    todo!()
}
