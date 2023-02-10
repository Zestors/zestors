//! # todo
//!
//! | __<--__ [`inboxes`] | [`distribution`] __-->__ |
//! |---|---|
mod channel_spec;
mod dyn_spec;
mod child_spec;
mod on_start_spec;
mod one_for_one_spec;
mod restart_limiter;
mod supervisor;
mod specification;
pub use channel_spec::*;
pub use dyn_spec::*;
pub use child_spec::*;
use futures::future::BoxFuture;
pub use on_start_spec::*;
pub use one_for_one_spec::*;
pub use restart_limiter::*;
pub use supervisor::*;
pub use specification::*;

use crate as zestors;
#[allow(unused)]
use crate::*;
use std::error::Error;

type BoxError = Box<dyn Error + Send>;
type BoxResult<T> = Result<T, BoxError>;

enum SpecStateWith<S: Specification> {
    Dead(S),
    Starting(S::Fut),
    Alive(S::Supervisee, S::Ref),
    Unhandled(BoxError),
    Finished,
}

enum SpecState<S: Specification> {
    Dead(S),
    Starting(S::Fut),
    Alive(S::Supervisee),
    Unhandled(BoxError),
    Finished,
}
