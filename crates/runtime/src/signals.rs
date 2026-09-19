use crate::*;

#[derive(Message, Debug)]
#[zestors(interface_path = "zestors_interface")]
pub(crate) struct Shutdown;

#[derive(Message, Debug)]
#[zestors(interface_path = "zestors_interface")]
pub(crate) struct Suspend;

#[derive(Message, Debug)]
#[zestors(interface_path = "zestors_interface")]
pub(crate) struct Resume;

#[derive(Message, Debug)]
#[zestors(interface_path = "zestors_interface")]
#[msg(reply = ())]
pub(crate) struct Ping;

#[derive(Interface, Debug)]
#[zestors(interface_path = "zestors_interface")]
pub(crate) enum SignalInterface {
    Shutdown(Envelope<Shutdown>),
    Suspend(Envelope<Suspend>),
    Resume(Envelope<Resume>),
    Ping(Envelope<Ping>),
}

/// A signal that can be sent to an actor to control its behavior. Signals take
/// precedence over messages, and are processed before any messages in the
/// actor's queue: see [`Inbox::recv_event`]/[`Inbox::recv_event_always`] for
/// where a signal shows up in an actor's own event loop, and
/// [`ActorOps::signal_shutdown`]/[`ActorOps::signal_suspend`]/
/// [`ActorOps::signal_resume`] for sending one.
///
/// There's a fourth, internal signal - the one behind [`ActorOps::ping`] -
/// that never appears as a `Signal` here: it's always fully handled by the
/// channel itself (by replying immediately) before an actor's event loop
/// ever sees it, so from an actor's own perspective it's invisible.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, Serialize, Deserialize)]
pub enum Signal {
    /// Sent by [`ActorOps::signal_shutdown`]. Moves the actor to
    /// [`ActorStatus::Exiting`] once processed.
    Shutdown,
    /// Sent by [`ActorOps::signal_suspend`]. Moves the actor to
    /// [`ActorStatus::Suspended`] once processed.
    Suspend,
    /// Sent by [`ActorOps::signal_resume`]. Moves a suspended actor back to
    /// [`ActorStatus::Running`] once processed.
    Resume,
}

impl Signal {
    /// Returns `true` for [`Signal::Shutdown`].
    pub fn is_shutdown(&self) -> bool {
        matches!(self, Signal::Shutdown)
    }

    /// Returns `true` for [`Signal::Resume`].
    pub fn is_resume(&self) -> bool {
        matches!(self, Signal::Resume)
    }

    /// Returns `true` for [`Signal::Suspend`].
    pub fn is_suspend(&self) -> bool {
        matches!(self, Signal::Suspend)
    }
}
