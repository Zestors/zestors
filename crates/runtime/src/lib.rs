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
//!
//! # Examples
//!
//! ## Fire-and-forget
//!
//! A minimal actor: it accepts `()` messages (the simplest possible
//! [`Interface`]) and counts how many it has seen.
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
//! // `watch_init` waits for the actor's first `recv`, so it's guaranteed
//! // to be running by the time we start sending it messages.
//! child.watch_init().await.unwrap();
//! for _ in 0..3 {
//!     child.cast(()).await.unwrap();
//! }
//!
//! // Ask it to shut down, then wait for it to actually exit and collect
//! // its result.
//! child.signal_shutdown();
//! assert_eq!(child.await.unwrap(), 3);
//! # }
//! ```
//!
//! ## Request/reply
//!
//! A real actor usually has more than one message, and expects a reply for
//! at least some of them - both come from its [`Interface`], generated with
//! `#[derive(Interface)]` over a set of `#[derive(Message)]` types rather
//! than written by hand (`GetCount`'s `#[msg(reply = u32)]` is what makes
//! [`Accepts::call`] return a `u32`, instead of `cast`'s fire-and-forget
//! `()`):
//!
//! ```
//! # use zestors::interface::{Envelope, Interface, Message};
//! # use zestors::runtime::prelude::*;
//! # use zestors::runtime::spawn_rand;
//! #[derive(Message, Debug)]
//! # #[zestors(interface_path = "zestors::interface")]
//! struct Increment;
//!
//! #[derive(Message, Debug)]
//! #[msg(reply = u32)]
//! # #[zestors(interface_path = "zestors::interface")]
//! struct GetCount;
//!
//! #[derive(Interface, Debug)]
//! # #[zestors(interface_path = "zestors::interface")]
//! enum CounterInterface {
//!     Increment(Envelope<Increment>),
//!     GetCount(Envelope<GetCount>),
//! }
//!
//! # #[tokio::main]
//! # async fn main() {
//! let child = spawn_rand(|mut inbox: Inbox<CounterInterface>| async move {
//!     let mut count = 0;
//!     while let Some(msg) = inbox.recv().await {
//!         match msg {
//!             CounterInterface::Increment(_) => count += 1,
//!             CounterInterface::GetCount(envelope) => {
//!                 let _ = envelope.reply(count);
//!             }
//!         }
//!     }
//!     Ok(())
//! });
//!
//! for _ in 0..5 {
//!     child.cast(Increment).await.unwrap();
//! }
//! // Every message goes through the same queue, in order, so `call` can
//! // only resolve once all 5 `Increment`s above have already been handled.
//! assert_eq!(child.call(GetCount).await.unwrap(), 5);
//!
//! child.signal_shutdown();
//! # }
//! ```
//!
//! ## Looking an actor up by `Pid`, and a bounded shutdown
//!
//! Actors don't have to be wired together by holding onto references
//! directly: any code that knows an actor's [`Pid`] can look it up through
//! the global [`Registry`], from anywhere in the process.
//!
//! ```
//! # use std::time::Duration;
//! # use zestors::runtime::prelude::*;
//! # use zestors::runtime::spawn;
//! # #[tokio::main]
//! # async fn main() {
//! let pid = Pid::new("counter");
//! let child = spawn(pid.clone(), |mut inbox: Inbox<()>| async move {
//!     while inbox.recv().await.is_some() {}
//!     Ok(())
//! })
//! .unwrap();
//!
//! // Elsewhere in the process, with only the `Pid` in hand:
//! let found = pid.typed_address::<()>().expect("still registered");
//! found.cast(()).await.unwrap();
//!
//! // `shutdown_abort` signals a shutdown and gives the actor a grace
//! // period to exit on its own, aborting it only if it overruns that.
//! child.shutdown_abort(Duration::from_secs(1)).await.unwrap();
//! assert!(pid.address().is_none(), "deregistered once fully gone");
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
