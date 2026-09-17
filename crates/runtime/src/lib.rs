//! The actor runtime underlying `zestors`.
//!
//! An actor is a [`tokio`] task, spawned via [`spawn`]/[`spawn_with`] (or, for a
//! signal-only task, [`spawn_task`]/[`spawn_task_with`]), that owns an [`Inbox`]
//! and receives messages and [`Signal`]s through it. Every actor is backed by a
//! [`Channel`], reachable through a family of reference types with different
//! ownership semantics:
//!
//! - [`Channel`] / [`StrongAddress`] keep the actor's channel alive and allow
//!   spawning a new task on it once the previous one has exited.
//! - [`Inbox`] is the strong reference held by the running task itself, used to
//!   receive messages and signals.
//! - [`Child`] owns the [`tokio::task::JoinHandle`] together with a
//!   [`StrongAddress`], and aborts the task when dropped unless detached.
//! - [`Address`] is a weak reference that can send messages and signals but does
//!   not keep the actor alive.
//!
//! Actors are looked up process-wide by [`Pid`] through the global [`Registry`].

use concurrent_queue::{ConcurrentQueue, PopError, PushError};
pub(crate) use rootcause::Report;
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use std::future::Future;
use std::time::Duration;
use std::{pin::pin, sync::Arc};
use tokio::sync::Notify;
pub(crate) use zestors_interface::*;

pub mod prelude {
    pub use crate::{
        ActorOps as _, Address, Cast as _, Child, Inbox, InboxEvent, IntoDyn as _, Pid, Signal,
        StrongAddress, spawn_with,
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
pub use channel::*;

pub mod errors;
pub(crate) use errors::*;

mod context;
mod registry;
mod signals;

pub use {context::*, registry::*, signals::*};
