mod channel;
mod registry;
mod signals;

pub use {channel::*, registry::*, signals::*};

#[allow(unused_imports)]
pub(crate) mod _prelude {
    pub(crate) use crate::{channel::*, registry::*, signals::*};
    pub(crate) use rootcause::Report;
    pub(crate) use serde::{Deserialize, Serialize};
    pub(crate) use std::{future::Future, time::Duration};
    pub(crate) use zestors_codegen::{Interface, Message};
    pub(crate) use zestors_interface::*;
}

pub mod prelude {
    pub use crate::{
        channel::{
            ActorOpsExt as _, Address, Child, Inbox, IntoDyn as _, Pid, Sends as _, StrongAddress,
            spawn_with,
        },
        signals::{InboxEvent, Signal},
    };
}
