mod actor_type;
mod address;
mod box_channel;
mod boxed_msg;
mod child;
mod child_pool;
mod errors;
pub(crate) mod gen;
mod protocol;
mod spawning;

pub use {
    actor_type::*, address::*, box_channel::*, boxed_msg::*, child::*, child_pool::*, errors::*,
    protocol::*, spawning::*,
};

pub use tiny_actor::{
    BackPressure, Capacity, Config, DynChannel, ExitError, HaltedError, Inbox, Link, Rcv,
    RecvError, SendError, Snd as SndRaw, SpawnError, TrySendError, TrySpawnError,
};
