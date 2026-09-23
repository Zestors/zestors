//! `zestors` is an actor framework for Rust with Erlang/OTP-style supervision
//! and clustering.
//!
//! An actor is a `tokio` task that owns an [`Inbox`](runtime::Inbox) and
//! handles the messages sent to it. Messages are plain types deriving
//! [`Message`](interface::Message), and the set of messages an actor accepts is
//! its [`Interface`](interface::Interface). Actors can be supervised, and nodes
//! can form a cluster in which an actor on another node is messaged like a
//! local one.
//!
//! **[The zestors book](https://zestors.github.io/zestors/)** is the guide:
//! it walks through messages, actors, their lifecycle, dynamic addresses,
//! supervision and the distributed mode, with examples. These API docs are the
//! reference.
//!
//! This crate re-exports the `zestors-*` crates as modules, and collects the
//! commonly used items, including the derive macros, in [`prelude`].
//!
//! | Module | What it provides |
//! |---|---|
//! | [`interface`] | [`Message`](interface::Message) and [`Interface`](interface::Interface): what an actor accepts and how it replies. |
//! | [`runtime`] | [`Inbox`](runtime::Inbox), [`Address`](runtime::Address), [`Child`](runtime::Child), [`Name`](runtime::Name) and the [`Registry`](runtime::Registry); signals, statuses, and [dynamic addresses](runtime::IntoDyn). |
//! | [`actor`] | [`Handler`](actor::Handler) (one handler per message) and [`Actor`](actor::Actor) (a full event loop), and [`Blueprint`](actor::Blueprint). |
//! | [`supervision`] | [`ChildSpec`](supervision::ChildSpec), [`ChildConfig`](supervision::ChildConfig), [`RestartIntensity`](supervision::RestartIntensity), and the [`GetChildren`](supervision::messages::GetChildren)/[`GetHealth`](supervision::messages::GetHealth) queries. |
//! | [`supervisor`] | The [`Supervisor`](supervisor::Supervisor) actor, and [`Node`](supervisor::Node) to run one as a program. |
//! | [`distr`] | Clustering: [`ClusterNode`](distr::ClusterNode), [`Cluster`](distr::Cluster), [`ClusterAddress`](distr::ClusterAddress), and messages with a [`StableId`](distr::StableId). |
//! | [`distr_quic`] | The QUIC transport for clusters, with mutual TLS: [`Quic`](distr_quic::Quic) and [`Tls`](distr_quic::Tls). |
//! | [`api_server`] | [`ApiServer`](api_server::ApiServer): an HTTP server for inspecting a running supervision tree. |
//!
//! # Example
//!
//! ```
//! use zestors::interface::{Envelope, Interface, Message};
//! use zestors::prelude::*;
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
//! child.cast(Increment).await.unwrap();
//! assert_eq!(child.call(GetCount).await.unwrap(), 1);
//! child.signal_shutdown();
//! # }
//! ```
//!
//! # Features
//!
//! - `auto-register`: enables `ClusterConfig::auto_register`, which registers
//!   every remote message in the binary.
#![cfg_attr(docsrs, feature(doc_cfg))]

/// The items you usually need: the derive macros, the reference types, and the
/// traits that provide their methods (`Accepts`, `ActorOps`, `ClusterAccepts`,
/// `ClusterActorOps`, …).
pub mod prelude {
    pub use zestors_actor::prelude::*;
    pub use zestors_codegen::{HandlerInterface, Interface, Message, StableId};
    pub use zestors_distr::prelude::*;
    pub use zestors_distr_quic::{Quic, Tls};
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

#[doc(inline)]
pub use zestors_distr_quic as distr_quic;
