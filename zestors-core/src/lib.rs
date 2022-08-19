mod boxed_msg;
mod box_channel;
mod protocol;
mod parts;
mod spawning;
pub(crate) mod gen;
pub mod accepts;
pub mod test;

pub use {
    boxed_msg::*,
    box_channel::*,
    protocol::*,
    parts::*,
    spawning::*,
    accepts::*,
    tiny_actor::{
        AnyChannel, BackPressure, Capacity, Channel, Config, DynChannel, ExitError, Growth,
        HaltedError, Inbox, Link, Rcv, RecvError, SendError, Snd as SndRaw, SpawnError,
        TrySendError, TrySpawnError,
    },
};

pub(crate) use tiny_actor::{
    Address as InnerAddress, Child as InnerChild, ChildPool as InnerChildPool
};