use concurrent_queue::{ConcurrentQueue, PopError, PushError};
pub(crate) use rootcause::Report;
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use std::future::Future;
use std::time::Duration;
use std::{pin::pin, sync::Arc};
use tokio::sync::Notify;
pub(crate) use zestors_interface::*;

pub mod prelude {
    pub use crate::{
        ActorOpsExt as _, Address, Child, Inbox, InboxEvent, IntoDyn as _, Pid, Sends as _, Signal,
        StrongAddress, spawn_with,
    };
}

const BACKPRESSURE_LIMIT: usize = 100;
const KEEP_N_SPAWNS: usize = 5;
const KEEP_N_EXITS: usize = 5;
const SIGNAL_QUEUE_CAPACITY: usize = 1_000_000;
const MSG_QUEUE_CAPACITY: usize = 1_000_000;

mod references;
pub use references::*;

mod ops;
pub use ops::*;

mod queue;
pub use queue::*;

mod dyn_conv;
pub use dyn_conv::*;

mod status;
pub use status::*;

mod spawn;
pub use spawn::*;

mod data;
pub use data::*;

mod task_box;
pub use task_box::*;

mod sends;
pub use sends::*;

pub mod errors;
pub(crate) use errors::*;

mod context;
mod registry;
mod signals;

pub use {context::*, registry::*, signals::*};
