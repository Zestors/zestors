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

mod combinator_specs;
mod restart_limiter;
mod child_spec;
mod startable;
mod supervisor;
pub use combinator_specs::*;
use futures::Future;
pub(super) use restart_limiter::*;
pub use child_spec::*;
pub use startable::*;
pub use supervisor::*;

#[allow(unused)]
use crate::all::*;
use std::error::Error;


//------------------------------------------------------------------------------------------------
//  Private types
//------------------------------------------------------------------------------------------------

type BoxError = Box<dyn Error + Send>;

enum SpecStateWith<S: Specifies> {
    Dead(S),
    Starting(S::Fut),
    Alive(S::Supervisee, S::Ref),
    Unhandled(BoxError),
    Finished,
}

enum SpecState<S: Specifies> {
    Dead(S),
    Starting(S::Fut),
    Alive(S::Supervisee),
    Unhandled(BoxError),
    Finished,
}

fn test() {
    let var = "String".to_string();
    let (_fut_a, _fut_b) = fn_once(|a| async { a + &var });
    let (_fut_a, _fut_b) = fn_once(|a| async { a + &var });
    // not allowed: drop(var); -> Only static variables as reference
    let var = "String".to_string();
    let (_fut_a, _fut_b) = fn_mut(|a| async { a + &var });
    let (_fut_a, _fut_b) = fn_mut(|a| async { a + &var });
    // not allowed: drop(var); -> Only static variables as reference

    let var = "String".to_string();
    let (_fut_a, _fut_b) = fn_once(|a| async {
        let var = var;
        a + &var
    });
}

fn fn_once<Fut: Future<Output = String>>(fun: impl Clone + FnOnce(String) -> Fut) -> (Fut, Fut) {
    (
        (fun.clone())("hi".to_string()),
        (fun.clone())("hi".to_string()),
    )
}

fn fn_mut<Fut: Future<Output = String>>(mut fun: impl FnMut(String) -> Fut) -> (Fut, Fut) {
    (fun("hi".to_string()), fun("hi".to_string()))
}
