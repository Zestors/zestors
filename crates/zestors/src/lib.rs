//! `zestors` is an actor framework with Erlang/OTP-style supervision.
//!
//! This crate is a facade: it re-exports the workspace crates as modules and
//! collects the commonly used items in [`prelude`].
//!
//! # Reading Order
//!
//! | Module | What it provides |
//! |---|---|
//! | [`interface`] | The vocabulary: derive `Message` for each message type and `Interface` for the set of messages an actor accepts. |
//! | [`runtime`] | The delivery machinery: actors receive through `Inbox`, everyone else sends through `Address`/`StrongAddress`, and actors are looked up by `Pid` in `Registry`. |
//! | [`actor`] | The actors: implement `Handler` (per-message handlers) or `Actor` (the full event loop), then package either in an `Blueprint`. |
//! | [`supervision`] | The supervisor's vocabulary: `ChildSpec` pairs a blueprint with its `ChildConfig`, and `RestartIntensity` bounds how often a child may restart. |
//! | [`supervisor`] | The supervision actors: `Supervisor` starts, watches, and restarts children per a `SupervisionStrategy`; `Node` runs a root supervisor as a whole program. |
//! | [`distr`] | Distributed messaging: nodes form a cluster, and `RemoteAddress` sends messages to actors on other nodes. `StableId` gives each message type a stable, globally unique `Id`. |
//! | [`api_server`] | HTTP introspection: `ApiServer` exposes `/processes`, `/snapshots`, and `/health` for a running tree. |
//! | `zestors_inspector` | The `zestors-inspector` GUI — a separate workspace crate, not re-exported here — visualizes the same tree data that [`api_server`] exposes. |
//!
//! # Guide: writing your first actor
//!
//! This walks through building a small actor step by step, then covers the
//! lifecycle behavior that governs it, a declarative alternative for
//! defining the actor body, and the supervision layer. Some of the
//! lifecycle behavior is easy to get wrong without knowing about it in
//! advance, so it's called out explicitly rather than left to be discovered
//! from a bug report.
//!
//! ## 1. Define your messages
//!
//! A message is a plain struct deriving [`Message`](interface::Message).
//! Messages come in two flavors: fire-and-forget, and request/reply.
//!
//! ```
//! use zestors::interface::Message;
//!
//! // Fire-and-forget: nothing is returned to the sender.
//! #[derive(Message, Debug)]
//! struct Increment;
//!
//! // Request/reply: `reply = u32` means calling this message returns a `u32`.
//! #[derive(Message, Debug)]
//! #[msg(reply = u32)]
//! struct GetCount;
//! ```
//!
//! ## 2. Group them into an `Interface`
//!
//! An actor doesn't accept a single message type - it accepts a set of
//! them, described by an [`Interface`](interface::Interface). Each variant
//! wraps one message type in an [`Envelope`](interface::Envelope):
//!
//! ```
//! use zestors::interface::{Envelope, Interface, Message};
//! #
//! # #[derive(Message, Debug)]
//! # struct Increment;
//! #
//! # #[derive(Message, Debug)]
//! # #[msg(reply = u32)]
//! # struct GetCount;
//!
//! #[derive(Interface, Debug)]
//! enum CounterInterface {
//!     Increment(Envelope<Increment>),
//!     GetCount(Envelope<GetCount>),
//! }
//! ```
//!
//! Only tuple variants with exactly one `Envelope<T>` field are allowed -
//! the derive macro needs that shape to generate the conversions that let a
//! channel accept and dispatch each message type.
//!
//! ## 3. Write the actor, spawn it, and send it messages
//!
//! The actor body is an `async` function (or closure) that owns an
//! [`Inbox`](runtime::Inbox) and loops over incoming messages until it
//! decides to stop. [`spawn_rand`](runtime::spawn_rand) starts it on a
//! fresh, randomly-generated [`Pid`](runtime::Pid), returning a
//! [`Child`](runtime::Child) that's both a handle for sending messages and
//! a future that resolves to the actor's own return value once it exits.
//!
//! ```
//! use zestors::interface::{Envelope, Interface, Message};
//! use zestors::runtime::prelude::*;
//! use zestors::runtime::spawn_rand;
//!
//! #[derive(Message, Debug)]
//! struct Increment;
//!
//! #[derive(Message, Debug)]
//! #[msg(reply = u32)]
//! struct GetCount;
//!
//! #[derive(Interface, Debug)]
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
//! // `cast` sends a message without waiting for it to be handled.
//! for _ in 0..5 {
//!     child.cast(Increment).await.unwrap();
//! }
//!
//! // `call` sends a message and waits for its reply. Every message to one
//! // actor goes through the same queue, in order, so this only resolves
//! // once the 5 `Increment`s above have already been processed.
//! assert_eq!(child.call(GetCount).await.unwrap(), 5);
//!
//! child.signal_shutdown();
//! # }
//! ```
//!
//! `envelope.reply(value)` answers a request/reply message; a
//! fire-and-forget message like `Increment` has nothing to reply to, so its
//! envelope is just dropped once handled.
//!
//! Dropping a `Child` aborts the actor it belongs to, unless `.detach()` has
//! been called on it first (or it's been consumed via `.into_handle()`).
//! This applies even if the `Child` is bound to `_` or simply goes out of
//! scope.
//!
//! ## 4. The actor lifecycle
//!
//! Every actor moves through a small set of states, tracked as
//! [`ActorStatus`](runtime::ActorStatus): `Initializing`, then `Running`,
//! optionally toggling between `Running` and `Suspended`, then `Exiting`,
//! and finally `Exited` (with a reason: normal exit, panic, abort, or an
//! error returned from the actor's own code).
//!
//! An actor reaches `Initializing` the moment it's spawned, before its task
//! has run even once, and only becomes `Running` once it calls a receiving
//! method for the first time. Code that runs immediately after spawning
//! should not assume the actor is already `Running`; either wait for it
//! with `child.watch_init().await`, or accept `Initializing` as well.
//!
//! Two behaviors around signals (shutdown, suspend, resume) depart from
//! what the method names alone would suggest:
//!
//! - **Sending a signal does not take effect immediately.** Calling
//!   `child.signal_shutdown()` adds "shutdown" to the actor's queue and
//!   returns without waiting for it to be processed. Checking
//!   `child.status()` on the next line can still show the previous status.
//!   To observe the effect, await something that actually waits for it,
//!   such as `child.watch_exit().await`.
//! - **Signals take priority over messages, except when the queue is
//!   empty.** Signals are checked before regular messages, so a shutdown
//!   request does not wait behind a backlog of messages. But once an
//!   actor's message queue is empty, processing a shutdown signal makes it
//!   exit immediately, without checking whether another signal is queued
//!   behind it. This matters only in specific timing situations (for
//!   example, pinging an actor in the same breath as shutting it down). An
//!   actor that needs to keep responding to signals after `Shutdown` should
//!   loop with `recv_event_always` and decide for itself when to stop,
//!   rather than relying on `recv`/`recv_event` to do it.
//!
//! ## 5. Declarative actors with `Handler`
//!
//! Writing the event loop by hand, as in step 3, gives full control over
//! how messages, signals, and other futures are read and interleaved.
//! [`Handler`](actor::Handler) is a higher-level alternative for actors that
//! don't need that control: it provides the event loop, and asks only for
//! lifecycle hooks and one [`Handle<M>`](actor::Handle) implementation per
//! message type.
//!
//! ```
//! use zestors::interface::{Envelope, Interface, Message, Request};
//! use zestors::prelude::*;
//!
//! #[derive(Message, Debug)]
//! struct Increment;
//!
//! #[derive(Message, Debug)]
//! #[msg(reply = u32)]
//! struct GetCount;
//!
//! #[derive(Interface, HandlerInterface, Debug)]
//! enum CounterInterface {
//!     Increment(Envelope<Increment>),
//!     GetCount(Envelope<GetCount>),
//! }
//!
//! #[derive(Debug, Clone)]
//! struct Counter {
//!     count: u32,
//! }
//!
//! impl Handler for Counter {
//!     type Interface = CounterInterface;
//! }
//!
//! impl Handle<Increment> for Counter {
//!     async fn handle(
//!         &mut self,
//!         _ctx: HandlerContext<'_, Self>,
//!         _msg: Increment,
//!         _req: (),
//!     ) -> Result<(), rootcause::Report> {
//!         self.count += 1;
//!         Ok(())
//!     }
//! }
//!
//! impl Handle<GetCount> for Counter {
//!     async fn handle(
//!         &mut self,
//!         _ctx: HandlerContext<'_, Self>,
//!         _msg: GetCount,
//!         req: Request<u32>,
//!     ) -> Result<(), rootcause::Report> {
//!         let _ = req.reply(self.count);
//!         Ok(())
//!     }
//! }
//!
//! # #[tokio::main]
//! # async fn main() {
//! // `ActorExt::spawn_rand` (from the `Actor` implementation `Handler`
//! // provides automatically) plays the same role as `spawn_rand` did in
//! // step 3.
//! let child = Counter { count: 0 }.spawn_rand();
//!
//! for _ in 0..5 {
//!     child.cast(Increment).await.unwrap();
//! }
//! assert_eq!(child.call(GetCount).await.unwrap(), 5);
//!
//! child.signal_shutdown();
//! # }
//! ```
//!
//! `#[derive(Interface, HandlerInterface)]` is what connects each message
//! variant to its `Handle<M>` implementation; both derives read the same
//! enum. A type implementing `Handler` implements [`Actor`](actor::Actor)
//! automatically, so it's spawned, sent messages, and awaited the same way
//! as any other actor.
//!
//! ## 6. Supervision
//!
//! Everything above is sufficient for actors that talk to each other
//! directly, without any restart policy. [`supervision`]/[`supervisor`] add
//! an optional layer for Erlang/OTP-style restart trees:
//! [`ChildSpec`](supervision::ChildSpec) pairs a blueprint with the
//! configuration a supervisor applies to it (restart mode, timeouts), and a
//! [`Supervisor`](supervisor::Supervisor) actor starts, watches, and
//! restarts a set of them.
//!
//! Any actor that is `Clone + Debug` - a [`Handler`](actor::Handler)
//! usually is - implements [`Blueprint`](actor::Blueprint)
//! automatically, so it can go directly into a `ChildSpec`, and several of
//! them into a [`SupervisorBlueprint`](supervisor::SupervisorBlueprint):
//!
//! [`Node`](supervisor::Node) covers the common case of running one root
//! supervisor as an entire program: starting it, restarting it on failure up
//! to a configured budget, and shutting it down gracefully on
//! Ctrl+C/SIGTERM.
//!
//! ```
//! use zestors::interface::{Envelope, Interface, Message};
//! use zestors::prelude::*;
//! use zestors::supervision::messages::GetChildren;
//! use zestors::supervisor::{Node, Supervisor};
//!
//! #[derive(Message, Debug)]
//! struct Ping;
//!
//! #[derive(Interface, HandlerInterface, Debug)]
//! enum WorkerInterface {
//!     Ping(Envelope<Ping>),
//! }
//!
//! #[derive(Debug, Clone)]
//! struct Worker;
//!
//! impl Handler for Worker {
//!     type Interface = WorkerInterface;
//! }
//!
//! impl Handle<Ping> for Worker {
//!     async fn handle(
//!         &mut self,
//!         _ctx: HandlerContext<'_, Self>,
//!         _msg: Ping,
//!         _req: (),
//!     ) -> Result<(), rootcause::Report> {
//!         Ok(())
//!     }
//! }
//!
//! # #[tokio::main]
//! # async fn main() {
//! let node = Node::new(
//!     Supervisor::blueprint()
//!         .child(Worker.pid("worker").unwrap())
//!         .rand_pid(),
//! );
//!
//! // In a real program, `node.run()` is usually just awaited directly from
//! // `main`. It's spawned onto its own task here only so the rest of this
//! // example can also demonstrate signaling and observing its shutdown.
//! let root = node.root_supervisor().address().clone();
//! let node_task = tokio::spawn(node.run());
//!
//! root.watch_running().await;
//!
//! let children = root.call(GetChildren).await.unwrap();
//! assert_eq!(children.len(), 1);
//!
//! // The root supervisor exiting on its own is a normal, successful stop
//! // for the whole node - triggered here with an ordinary shutdown signal,
//! // in place of the Ctrl+C/SIGTERM a real deployment would send.
//! root.signal_shutdown();
//! assert!(node_task.await.unwrap().is_ok());
//! # }
//! ```
//!
//! ## Where to go next
//!
//! The [`runtime`], [`interface`], [`actor`], [`supervision`], and
//! [`supervisor`] crate docs each have further worked examples - a bare
//! `Inbox<()>` actor, looking an actor up by `Pid` from elsewhere in the
//! process, the lower-level `Envelope`/`Interface` machinery this guide
//! builds on, and more. Every example in this guide and in those crate docs
//! compiles and runs as part of this workspace's test suite, so they stay
//! in sync with the framework's actual behavior rather than drifting from
//! it over time.

/// The items you usually need, re-exported from each sub-crate plus the
/// codegen macros.
pub mod prelude {
    pub use zestors_actor::prelude::*;
    #[expect(unused_imports)]
    pub use zestors_api_server::prelude::*;
    pub use zestors_codegen::{HandlerInterface, Interface, Message, StableId};
    pub use zestors_distr::prelude::*;
    pub use zestors_interface::prelude::*;
    pub use zestors_runtime::prelude::*;
    pub use zestors_supervision::prelude::*;
    pub use zestors_supervisor::prelude::*;
}

#[doc(inline)]
pub use zestors_api_server as api_server;

#[doc(inline)]
pub use zestors_actor as actor;

#[doc(inline)]
pub use zestors_interface as interface;

#[doc(inline)]
pub use zestors_runtime as runtime;

#[doc(inline)]
pub use zestors_supervision as supervision;

#[doc(inline)]
pub use zestors_supervisor as supervisor;

#[doc(inline)]
pub use zestors_distr as distr;
