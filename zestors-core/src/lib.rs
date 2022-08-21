mod address;
mod address_type;
mod box_channel;
mod boxed_msg;
mod errors;
pub(crate) mod gen;
mod child;
mod protocol;
mod spawning;

pub use {
    address::*, address_type::*, box_channel::*, boxed_msg::*, errors::*, child::*, protocol::*,
    spawning::*,
};

pub use tiny_actor::{
    BackPressure, Capacity, Config, DynChannel, ExitError, Growth, HaltedError, Inbox, Link, Rcv,
    RecvError, SendError, Snd as SndRaw, SpawnError, TrySendError, TrySpawnError,
};
