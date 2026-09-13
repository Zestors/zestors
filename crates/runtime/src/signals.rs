use crate::_prelude::*;

#[derive(Message, Debug)]
#[msg(path = "zestors_interface")]
pub(crate) struct Shutdown;

#[derive(Message, Debug)]
#[msg(path = "zestors_interface")]
pub(crate) struct Suspend;

#[derive(Message, Debug)]
#[msg(path = "zestors_interface")]
pub(crate) struct Resume;

#[derive(Message, Debug)]
#[msg(path = "zestors_interface")]
#[msg(reply = ())]
pub(crate) struct Ping;

#[derive(Interface, Debug)]
#[interface(path = "zestors_interface")]
pub(crate) enum SignalInterface {
    Shutdown(Envelope<Shutdown>),
    Suspend(Envelope<Suspend>),
    Resume(Envelope<Resume>),
    Ping(Envelope<Ping>),
}

/// A signal that can be sent to an actor to control its behavior. Signals take
/// precedence over messages, and are processed before any messages in the actor's queue.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, Serialize, Deserialize)]
pub enum Signal {
    Shutdown,
    Suspend,
    Resume,
}

impl Signal {
    pub fn is_shutdown(&self) -> bool {
        matches!(self, Signal::Shutdown)
    }

    pub fn is_resume(&self) -> bool {
        matches!(self, Signal::Resume)
    }

    pub fn is_suspend(&self) -> bool {
        matches!(self, Signal::Suspend)
    }
}
