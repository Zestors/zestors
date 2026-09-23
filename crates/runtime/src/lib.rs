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
//!   [`StrongAddress`], and **aborts the task when dropped** unless detached.
//!
//! [`Accepts`] sends messages through any of them, and [`ActorOps`] provides the
//! rest: signals, [`ActorStatus`] and the `monitor_*` methods to wait for one,
//! and inspection. Import both through `zestors::prelude`.
//!
//! Actors are looked up by [`Name`] through the [`Registry`], which covers one
//! process. Actors on other nodes of a cluster are reached through
//! `zestors-distr` instead.
//!
//! # Dynamic addresses
//!
//! A reference's type parameter is its [`Context`]: either the actor's full
//! [`Interface`], or a [`Dyn`] set of messages it is known to accept, like
//! `Address<Dyn<(Ping, GetHealth)>>`. References to different kinds of actor
//! that share messages then have the same type. [`IntoDyn`] and [`AsDyn`]
//! convert between contexts, checked at compile time or at runtime. The
//! [zestors book](https://zestors.github.io/zestors/dynamic-addresses.html)
//! explains this in detail.
//!
//! # Examples
//!
//! ## An actor, and its result
//!
//! The simplest actor accepts only `()` messages. Here it counts them, and
//! returns the count when it is shut down.
//!
//! ```
//! # use zestors::runtime::prelude::*;
//! # use zestors::runtime::spawn_rand;
//! # #[tokio::main]
//! # async fn main() {
//! let child = spawn_rand(|mut inbox: Inbox<()>| async move {
//!     let mut count = 0;
//!     while inbox.recv().await.is_some() {
//!         count += 1;
//!     }
//!     Ok::<_, rootcause::Report>(count)
//! });
//!
//! for _ in 0..3 {
//!     child.cast(()).await.unwrap();
//! }
//!
//! // Ask it to shut down, then wait for it to exit and collect its result.
//! child.signal_shutdown();
//! assert_eq!(child.await.unwrap(), 3);
//! # }
//! ```
//!
//! ## Looking an actor up by `Name`, and a bounded shutdown
//!
//! ```
//! # use std::time::Duration;
//! # use zestors::runtime::prelude::*;
//! # use zestors::runtime::{Registry, spawn};
//! # #[tokio::main]
//! # async fn main() {
//! let name = Name::new("counter");
//! let child = spawn(name.clone(), |mut inbox: Inbox<()>| async move {
//!     while inbox.recv().await.is_some() {}
//!     Ok(())
//! })
//! .unwrap();
//!
//! // Elsewhere in the process, with only the `Name` in hand:
//! let found = Registry::local()
//!     .get_typed::<()>(&name)
//!     .expect("still registered");
//! found.cast(()).await.unwrap();
//!
//! // `shutdown_abort` signals a shutdown and gives the actor a grace
//! // period to exit on its own, aborting it only if it overruns that.
//! child.shutdown_abort(Duration::from_secs(1)).await.unwrap();
//! assert!(name.address().is_none(), "deregistered once fully gone");
//! # }
//! ```

use concurrent_queue::{ConcurrentQueue, PopError, PushError};
pub(crate) use rootcause::Report;
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use std::future::Future;
use std::pin::pin;
use std::time::Duration;
use tokio::sync::Notify;
pub(crate) use zestors_interface::*;

#[doc(hidden)]
pub mod prelude {
    pub use crate::{
        Accepts as _, ActorOps as _, Address, AsDyn as _, Child, Inbox, InboxEvent, IntoDyn as _,
        Name, Signal, StrongAddress, spawn,
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
