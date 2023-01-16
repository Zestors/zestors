//! Zestors is a dynamic actor-framework built for Rust applications.
//! 
//! # Documentation
//! All modules are self-documented. For a new user it is recommended to read the documentation in 
//! the following order.
//! - [`messaging`] : This explains the basis of how messaging works within zestors. It explains how
//! what protocols and messages are and how to define new ones.
//! - [`child`] : This explains what children are, and how a child can be supervised and shut down
//! manually.
//! - [`inbox`] : This explains the concept of different actor-inboxes.
//! - [`channel`] : 
//! - [`spawning`] : How to spawn an actor.
//! - [`config`] : The configuration of actors.
//! - [`supervision`] : todo
//! - [`distribution`] : todo
//! 
//! 
pub(crate) mod _priv;
pub(crate) use _priv::*;
pub mod messaging;
#[doc(inline)]
pub use messaging::{
    protocol,
    request::{Rx, Tx},
    Message, Protocol,
};

pub mod config;
#[doc(inline)]
pub use config::{BackPressure, Capacity, Config, Link};

pub mod spawning;
#[doc(inline)]
pub use spawning::{spawn, spawn_many, spawn_one};

pub mod child;
#[doc(inline)]
pub use child::{Child, ChildGroup};

pub mod inbox;
#[doc(inline)]
pub use inbox::{basic::Inbox, halter::Halter};

pub mod actor_kind;
#[doc(inline)]
pub use actor_kind::{Accept, Accepts};

pub mod channel;
#[doc(inline)]
pub use channel::{ActorId, ActorRef};

/// # TODO
pub mod distribution;

/// # TODO
pub mod supervision;

pub mod all {
    pub use crate::actor_kind::*;
    pub use crate::channel::*;
    pub use crate::child::*;
    pub use crate::config::*;
    pub use crate::inbox::{basic::*, halter::*, *};
    pub use crate::messaging::*;
    pub use crate::spawning::*;
    pub use crate::supervision::*;
}
