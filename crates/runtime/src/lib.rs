pub mod channel;
pub mod registry;
pub mod signals;

pub use channel::{spawn, spawn_task, spawn_task_with, spawn_with};

#[allow(unused_imports)]
pub(crate) mod _prelude {
    pub(crate) use crate::{channel::*, registry::*, signals::*};
    pub(crate) use rootcause::Report;
    pub(crate) use serde::{Deserialize, Serialize};
    pub(crate) use std::{future::Future, time::Duration};
    pub(crate) use zestors_codegen::{Interface, Message};
    pub(crate) use zestors_messaging::*;
}

pub mod prelude {
    pub use crate::{
        channel::{
            ActorOpsExt as _, Address, Child, Inbox, IntoDyn as _, Pid, Sends as _, StrongAddress,
            spawn_with,
        },
        signals::{Event, Signal},
    };
}
