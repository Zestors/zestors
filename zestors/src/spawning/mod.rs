//! # Spawn functions
//! When spawning an actor, there are a couple of options to choose from:
//! - [`spawn(FnOnce)`](spawn) - This is the simplest way to spawn an actor and uses a default [`Link`] and
//! [`InboxType::Config`].
//! - [`spawn_with(link, cfg, FnOnce)`](spawn_with) - Same as `spawn`, but allows for a custom link and config.
//! - [`spawn_many(iter, FnOnce)`](spawn_many) - Same as `spawn`, but instead spawns many processes using
//! the iterator as the first argument to the function.
//! - [`spawn_many_with(iter, link, cfg, FnOnce)`](spawn_many_with) - Same as `spawn_many`, but allows for a
//! custom link and config.
//!
//! It is also possible to spawn more processes onto an actor that is already running with
//! [`ChildPool::spawn_onto`] and [`ChildPool::try_spawn_onto`].
//!
//! # Link
//! Every actor is spawned with a [`Link`] that indicates whether the actor is attached
//! or detached. By default a [`Link`] is attached with an abort-timer of 1 second; this means that when the
//! [`Child`] is dropped, the actor has 1 second to halt before it is aborted. If the link is detached, then
//! the child can be dropped without halting or aborting the actor.
//!
//! # Inbox configuration
//! Actors can be spawned with any [`InboxType`], where the [`InboxType::Config`] specifies the configuration-options
//! for that inbox. The config for an [`Inbox`] is [`Capacity`] and for a [`Halter`] it is `()`.
//!
//! The [`Capacity`] of an inbox can be one of three options:
//! - [`Capacity::Bounded(size)`](`Capacity::Bounded) --> An inbox that doesn't accept new messages after the
//! given size has been reached.
//! - [`Capacity::Unbounded`](Capacity::Unbounded) --> An inbox that grows in size infinitely when new messages
//! are received.
//! - [`Capacity::BackPressure(BackPressure)`](Capacity::BackPressure) (default) --> An unbounded inbox with
//! a [`BackPressure`] mechanic. An overflow of messages is handled by increasing the delay for sending a message.
//!
//! | __<--__ [`actor_type`](crate::actor_type) | [`handler`](crate::handler) __-->__ |
//! |---|---|
//!
//! # Example
//! ```
#![doc = include_str!("../../examples/spawning.rs")]
//! ```

mod capacity;
mod errors;
mod functions;
mod link;
#[allow(unused)]
use crate::all::*;
pub use {capacity::*, errors::*, functions::*, link::*};
