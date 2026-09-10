pub mod channel;
pub mod messaging;
pub mod registry;
pub mod signals;

pub use channel::{spawn, spawn_task, spawn_task_with, spawn_with};

#[allow(unused_imports)]
pub(crate) mod _prelude {
    pub(crate) use crate::{channel::*, messaging::*, registry::*, signals::*};
    pub(crate) use rootcause::Report;
    pub(crate) use serde::{Deserialize, Serialize};
    pub(crate) use std::{future::Future, time::Duration};
    pub(crate) use zestors_codegen::{Interface, Message};
}

pub mod prelude {
    pub use crate::{
        channel::{
            ActorOpsExt as _, Address, Child, Inbox, IntoDyn as _, Pid, StrongAddress, spawn_with,
        },
        messaging::{Envelope, Interface, Message, Sends as _},
        signals::{Event, Signal},
    };
}
