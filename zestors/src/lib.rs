/*!
Zestors is a dynamic actor-framework built for Rust applications.

# Documentation
All modules are self-documented. For a new user it is recommended to read the documentation in
the following order.
- [`messaging`] - Using a protocol to send messages.
- [`monitoring`] - Monitoring an actor.
- [`actor_type`] - Specifying the type of an actor.
- [`spawning`] - Spawning actors of different types.
- [`inboxes`] - Using different inboxes.
- [`supervision`] : todo
- [`distribution`] : todo
*/

pub mod messaging;
#[doc(inline)]
pub use messaging::{
    protocol,
    request::{Rx, Tx},
    IntoRecv, Message, Protocol,
};

pub mod spawning;
#[doc(inline)]
pub use spawning::{spawn, spawn_group, spawn_group_with, spawn_with, Link};

pub mod monitoring;
#[doc(inline)]
pub use monitoring::{ActorId, ActorRefExt, ActorRefExtDyn, Address, Child, ChildGroup};

pub mod inboxes;
#[doc(inline)]
pub use inboxes::{BackPressure, Capacity, MultiHalter, Inbox, Halter};

pub mod actor_type;
#[doc(inline)]
pub use actor_type::Accept;

pub mod distribution;

pub mod supervision;

pub mod all {
    pub use crate::actor_type::*;
    pub use crate::inboxes::{halter::*, inbox::*, *};
    pub use crate::messaging::*;
    pub use crate::monitoring::*;
    pub use crate::spawning::*;
    pub use crate::supervision::*;
}
pub(crate) mod _priv;
#[allow(unused)]
use crate::all::*;
