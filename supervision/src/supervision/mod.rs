/*!
# Supervisee lifecycle

#### __`(1)` Specification__:
Every supervisee starts out as a [`Specification`]. The supervisee is not running, and
can be started using [`Specification::start`]. This returns the [`Specification::StartFut`] `(2)`.

In this state the supervisee can also be queried for it's [`Specification::start_timeout`].

#### __`(2)` Starting__:
The supervisee is currently starting as a [`Specification::StartFut`]. If the future completes
successfully then we get a [`Supervisee`] `(3)` that can be supervised. If however the supervisee
fails to start, it returns one of three errors:
- [`StartError::Unrecoverable(Box<dyn Error + Send>)`] : An unrecoverable error occured during starting.
  This should backpropagate to supervisors until it can be handled or until the application
  as a whole shuts down `(5)`.
- [`StartError::Failed(Specification)`] : Starting has failed, and the original specification
  is returned `(1)`. This specification may be used to restart the supervisee.
- [`StartError::Completed`] : Starting has failed because the supervisee does not have
  any work left to be done and is therefore completed `(4)`.

If starting the supervisee takes longer than it's `start_timeout`, then the future may be
dropped `(5)`.

#### __`(3)` Supervisee__:
The supervisee is running and can be supervised by awaiting the [`Supervisee`].
The supervisee can exit successfully in two ways:
- `Some(Specification)` : The supervisee has exited, but is not yet completed. It would like to be
  restarted with it's [`Specification`] `(5)`.
- `None` : The supervisee has exited and has completed all it's task `(4)`.

The supervisee can also exit with a `Err(Box<dyn Error + Send>)`. This indicates an unrecoverable
error `(5)`.

The supervisee can be halted and aborted using [`Supervisee::halt`] and [`Supervisee::abort`]. The
supervisee also defines a [`Supervisee::abort_timeout`] which gives an upper limit how long the
supervisee needs to shut down after being halted; if the supervisee exceeds this limit, it may
be aborted or dropped `(5)`.

#### __`(4)` Completed__:
The supervisee has completed it's task successfully and can not be restarted.

#### __`(5)` Unrecoverable__:
The supervisee is not completed but is not able to restart either.

| __<--__ [`handler`](crate::handler) | [`distribution`](crate::distribution) __-->__ |
|---|---|
*/

mod child_spec;
mod combinators;
mod restart_limiter;
mod supervisor;
mod supervisor2;
mod traits;
mod traits_ext;
mod handler_spec;
pub use child_spec::*;
use futures::Future;
pub(super) use restart_limiter::*;
pub use {combinators::*, traits_ext::*, handler_spec::*};
pub use {traits::*}; // pub use supervisor::*;

#[allow(unused)]
use crate::all::*;
use std::{convert::Infallible, error::Error};

//------------------------------------------------------------------------------------------------
//  Private types
//------------------------------------------------------------------------------------------------


pub fn spec_from_start_function<Fut, E, A, Ref, Err>(
    function: impl FnOnce() -> Fut + Clone + Send,
) -> impl Specification<Ref = Ref>
where
    Fut: Future<Output = Result<(Child<E, A>, Ref), Err>> + Send,
    E: Send + 'static,
    A: ActorType,
    Ref: Send + 'static,
{
    TodoSpec(todo!())
}

// pub fn spec_from_startable<Fut, S, I>(
//     f: impl FnOnce() -> Fut + Clone + Send,
//     link: Link,
//     cfg: <S::InboxType as InboxType>::Config,
// ) -> impl Specification<Ref = S::Ref>
// where
//     Fut: Future<Output = I> + Send,
//     S: Startable<I>,
//     <S::InboxType as InboxType>::Config: Clone,
//     I: Send,
// {
//     spec_from_start_function(move || async move {
//         let init = f().await;
//         S::start_with(init, link, cfg).await
//     })
// }

// pub fn spec_from_spawn_function<E, A>(
//     function: impl FnOnce() -> (Child<E, A>, Address<A>) + Clone + Send,
// ) -> impl Specification<Ref = Address<A>>
// where
//     E: Send + 'static,
//     A: ActorType + 'static,
// {
//     spec_from_start_function(|| async move { Ok::<_, Infallible>(function()) })
// }

// pub fn spec_from_spawnable<S>(
//     init: S,
//     link: Link,
//     cfg: <S::InboxType as InboxType>::Config,
// ) -> impl Specification<Ref = Address<S::InboxType>>
// where
//     S: Spawnable + Clone + Send,
//     <S::InboxType as InboxType>::Config: Clone,
// {
//     spec_from_spawn_function(move || init.spawn_with(link, cfg))
// }

//------------------------------------------------------------------------------------------------
//  Todo
//------------------------------------------------------------------------------------------------

struct TodoSpec<T>(T);

impl<T: Send + 'static> Specification for TodoSpec<T> {
    type Ref = T;

    type Supervisee = TodoSpec<T>;

    fn start_supervised<'async_trait>(
        self,
    ) -> core::pin::Pin<
        Box<
            dyn core::future::Future<Output = StartResult<Self>>
                + core::marker::Send
                + 'async_trait,
        >,
    >
    where
        Self: 'async_trait,
    {
        todo!()
    }
}

impl<T: Send + 'static> Supervisee for TodoSpec<T> {
    type Spec = TodoSpec<T>;

    fn poll_supervise(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context,
    ) -> std::task::Poll<SupervisionResult<Self::Spec>> {
        todo!()
    }

    fn shutdown_time(self: std::pin::Pin<&Self>) -> std::time::Duration {
        todo!()
    }

    fn halt(self: std::pin::Pin<&mut Self>) {
        todo!()
    }

    fn abort(self: std::pin::Pin<&mut Self>) {
        todo!()
    }
}

