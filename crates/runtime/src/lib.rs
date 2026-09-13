mod channel;
mod context;
mod pid;
mod registry;
mod signals;

pub use {channel::*, context::*, pid::*, registry::*, signals::*};

#[allow(unused_imports)]
pub(crate) mod _prelude {
    pub(crate) use crate::*;
    pub(crate) use rootcause::Report;
    pub(crate) use serde::{Deserialize, Serialize};
    pub(crate) use std::{future::Future, time::Duration};
    pub(crate) use zestors_codegen::{Interface, Message};
    pub(crate) use zestors_interface::*;
}

pub mod prelude {
    pub use crate::{
        ActorOpsExt as _, Address, Child, Inbox, InboxEvent, IntoDyn as _, Pid, Sends as _, Signal,
        StrongAddress, spawn_with,
    };
}
