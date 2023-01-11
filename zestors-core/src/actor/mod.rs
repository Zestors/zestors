//! Module containing all the different parts of the actor and their spawn functions.
//! The most important of these are: [Child], [ChildPool], [Address] and [Inbox] with
//! [spawn], [spawn_one] and [spawn_many]. Ready documentation on their respective parts
//! for more information.
//!
//!
//! # Actor states
//!
//! An overview of similar methods callable on (addresses)[Address], (inboxes)[Inbox] and
//! (children)[Child]:
//! - `is_finished`: Returns true if the underlying tasks have finished. (Only callable on a child)
//! - `is_aborted`: Returns true if the actor has been aborted. (Only callable on a child)
//! - `has_exited`: Returns true if the inboxes have been dropped.
//! - `is_closed`: Returns true if the channel has been closed.

mod address;
mod child;
mod inbox;
mod shutdown;
mod spawning;
mod into;
mod errors;
mod halter;
mod defines_pool;

pub use address::*;
pub use child::*;
pub use inbox::*;
pub use shutdown::*;
pub use spawning::*;
pub use into::*;
pub use errors::*;
pub use halter::*;
pub use defines_pool::*;
