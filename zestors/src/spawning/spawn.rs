use crate::all::*;
use futures::Future;

/// Same as [`spawn_with`] but with a default [`Link`] and [`ActorInbox::Config`].
/// 
/// # Usage
/// ```
/// # tokio_test::block_on(main());
/// use zestors::{actor_type::inbox::Inbox, spawning::spawn};
/// 
/// # async fn main() {
/// let (child, address) = spawn(|inbox: Inbox<()>| async move {
///     todo!()
/// });
/// # }
/// ```
pub fn spawn<I, E, Fun, Fut>(function: Fun) -> (Child<E, I>, Address<I>)
where
    Fun: FnOnce(I) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: ActorInbox,
    I::Config: Default,
    E: Send + 'static,
{
    spawn_with(Default::default(), Default::default(), function)
}

/// Spawn an actor with the given function using the [`Link`] and [`ActorInbox::Config`].
/// 
/// # Usage
/// ```
/// # tokio_test::block_on(main());
/// use zestors::{actor_type::{Capacity, inbox::Inbox}, actor_ref::Link, spawning::spawn_with};
/// 
/// # async fn main() {
/// let (child, address) = spawn_with(
///     Link::default(), 
///     Capacity::default(), 
///     |inbox: Inbox<()>| async move {
///         todo!()
///     }
/// );
/// # }
/// ```
pub fn spawn_with<I, E, Fun, Fut>(
    link: Link,
    config: I::Config,
    function: Fun,
) -> (Child<E, I>, Address<I>)
where
    Fun: FnOnce(I) -> Fut + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: ActorInbox,
    E: Send + 'static,
{
    let (channel, inbox) = I::init_single_inbox(config, 1, ActorId::generate());
    // let inbox = I::from_channel(channel.clone());
    let handle = tokio::task::spawn(async move { function(inbox).await });
    (
        Child::new(channel.clone(), handle, link),
        Address::from_channel(channel),
    )
}

/// Spawn an actor consisting of multiple processes with the given function using a default
/// [`Link`] and [`ActorInbox::Config`].
///
/// # Usage
/// ```
/// # tokio_test::block_on(main());
/// use zestors::{actor_type::inbox::Inbox, spawning::spawn_many};
/// 
/// # async fn main() {
/// let (child, address) = spawn_many(0..5, |i: u32, inbox: Inbox<()>| async move {
///     todo!()
/// });
/// # }
/// ```
pub fn spawn_many<I, E, Itm, Fun, Fut>(
    iter: impl ExactSizeIterator<Item = Itm>,
    function: Fun,
) -> (ChildPool<E, I>, Address<I>)
where
    Fun: FnOnce(Itm, I) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: MultiProcessInbox,
    I::Config: Default,
    E: Send + 'static,
    Itm: Send + 'static,
{
    spawn_many_with(Default::default(), Default::default(), iter, function)
}

/// Spawn an actor consisting of multiple processes with the given function using a custom
/// [`Link`] and [`ActorInbox::Config`].
///
/// # Usage
/// ```
/// # tokio_test::block_on(main());
/// use zestors::{actor_type::{Capacity, inbox::Inbox}, actor_ref::Link, spawning::spawn_many_with};
/// 
/// # async fn main() {
/// let (child, address) = spawn_many_with(
///     Link::default(), 
///     Capacity::default(), 
///     0..5, 
///     |i: u32, inbox: Inbox<()>| async move {
///         todo!()
///     }
/// );
/// # }
/// ```
pub fn spawn_many_with<I, E, Itm, Fun, Fut>(
    link: Link,
    config: I::Config,
    iter: impl ExactSizeIterator<Item = Itm>,
    function: Fun,
) -> (ChildPool<E, I>, Address<I>)
where
    Fun: FnOnce(Itm, I) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = E> + Send,
    I: MultiProcessInbox,
    E: Send + 'static,
    Itm: Send + 'static,
{
    let channel = I::init_multi_inbox(config, iter.len(), 1, ActorId::generate());
    let handles = iter
        .map(|i| {
            let fun = function.clone();
            let inbox = I::from_channel(channel.clone());
            tokio::task::spawn(async move { fun(i, inbox).await })
        })
        .collect::<Vec<_>>();
    (
        Child::new(channel.clone(), handles, link),
        Address::from_channel(channel),
    )
}
