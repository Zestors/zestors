//! The actor runtime underlying `zestors`.
//!
//! An actor is a [`tokio`] task, spawned via [`spawn`]/[`spawn_rand`] (or, for a
//! signal-only task, [`spawn_task`]/[`spawn_task_rand`]), that owns an [`Inbox`]
//! and receives messages and [`Signal`]s through it. Every actor is backed by a
//! shared channel, reachable through a family of reference types with different
//! ownership semantics:
//!
//! - [`Address`] is a weak reference that can send messages and signals but
//!   does not keep the actor alive.
//! - [`StrongAddress`] additionally keeps the channel alive, and allows
//!   spawning a new task on it once the previous one has exited.
//! - [`Inbox`] is the strong reference held by the running task itself, used to
//!   receive messages and signals.
//! - [`Child`] owns the [`tokio::task::JoinHandle`] together with a
//!   [`StrongAddress`], and aborts the task when dropped unless detached.
//!
//! Actors are looked up process-wide by [`Pid`] through the global [`Registry`].
//!
//! Once you have a reference, [`Accepts`] and [`ActorOps`] (both re-exported
//! through the [`prelude`]) provide the methods for interacting with the
//! actor — sending and receiving messages, inspecting status, and more.

use concurrent_queue::{ConcurrentQueue, PopError, PushError};
pub(crate) use rootcause::Report;
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use std::future::Future;
use std::pin::pin;
use std::time::Duration;
use tokio::sync::Notify;
pub(crate) use zestors_interface::*;

pub mod prelude {
    pub use crate::{
        Accepts as _, ActorOps as _, Address, Child, Inbox, InboxEvent, IntoDyn as _, Pid, Signal,
        StrongAddress, spawn,
    };
}

const BACKPRESSURE_LIMIT: usize = 100;
const KEEP_N_SPAWNS: usize = 5;
const KEEP_N_EXITS: usize = 5;
const SIGNAL_QUEUE_CAPACITY: usize = 1_000_000;
const MSG_QUEUE_CAPACITY: usize = 1_000_000;

mod references;
pub use references::*;

mod queue;
pub(crate) use queue::*;

mod spawn;
pub use spawn::*;

mod channel;
pub(crate) use channel::*;

pub mod errors;
pub(crate) use errors::*;

mod accepts;
mod context;
mod ops;
mod registry;
mod signals;
mod status;

pub use {accepts::*, context::*, ops::*, registry::*, signals::*, status::*};
