use futures::Future;
use tiny_actor::Channel;

use crate::*;

pub fn spawn<P, E, Fun, Fut>(config: Config, fun: Fun) -> (Child<E, Channel<P>>, StaticAddress<P>)
where
    Fun: FnOnce(Inbox<P>) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send + 'static,
    E: Send + 'static,
    P: Send + 'static,
{
    let (child, addr) = tiny_actor::spawn(config, fun);
    (Child::from_tiny_child(child), StaticAddress::from_inner(addr))
}

// pub fn spawn_one<M, E, Fun, Fut>(
//     config: Config,
//     fun: Fun,
// ) -> (ChildPool<E, Channel<M>>, Address<Channel<M>>)
// where
//     Fun: FnOnce(Inbox<M>) -> Fut + Send + 'static,
//     Fut: Future<Output = E> + Send + 'static,
//     E: Send + 'static,
//     M: Send + 'static,
// {
//     let (child, addr) = tiny_actor::spawn_one(config, fun);
//     (Child::from_tiny_child(child), Address::from_inner(addr))
// }

// pub fn spawn_many<M, E, I, Fun, Fut>(
//     iter: impl IntoIterator<Item = I>,
//     config: Config,
//     fun: Fun,
// ) -> (ChildPool<E, Channel<M>>, Address<Channel<M>>)
// where
//     Fun: FnOnce(I, Inbox<M>) -> Fut + Send + 'static + Clone,
//     Fut: Future<Output = E> + Send + 'static,
//     E: Send + 'static,
//     M: Send + 'static,
//     I: Send + 'static,
// {
//     let (child, addr) = tiny_actor::spawn_many(config, fun);
//     (Child::from_tiny_child(child), Address::from_inner(addr))
// }
