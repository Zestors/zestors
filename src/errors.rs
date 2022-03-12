/// Error returned when trying to send a [Msg] or [Req] to an [Actor].
///
/// Sending failed because the [Actor] died before the [Msg] or [Req] could be sent.
#[derive(Debug)]
pub struct DidntArrive<T>(pub T);

/// Error returned when waiting for a [Reply] from an [Actor].
///
/// The [Actor] has died after the [Msg] was sent.
#[derive(Debug)]
pub struct NoReply;

#[derive(Debug)]
pub struct RequestDropped<T>(pub T);

/// Error returned when sending a [Msg] or [Req] to an [Actor] and then
/// trying to wait for the [Reply].
#[derive(Debug)]
pub enum ReqRecvError<T> {
    /// The [Actor] has died before the [Req] could be sent.
    DidntArrive(T),
    /// The [Actor] has died after the [Req] was sent, but before it could reply.
    NoReply,
}

/// Error returned when trying to receive a [Reply] from an [Actor].
#[derive(Debug)]
pub enum TryRecvError {
    /// The [Actor] has not yet sent back a [Reply], but is still alive.
    NoReplyYet,
    /// The [Actor] has died after the [Req] was sent, but before it could reply.
    NoReply,
}

#[derive(Debug)]
pub enum ProcessRefRequestError {
    NotRegistered,
    IncorrectActorType,
    NodeDisconnected
}

impl From<RegistryGetError> for ProcessRefRequestError {
    fn from(e: RegistryGetError) -> Self {
        match e {
            RegistryGetError::IdNotRegistered => Self::NotRegistered,
            RegistryGetError::IncorrectActorType => Self::IncorrectActorType,
        }
    }
}

//-------------------------------------
// Display & Error
//-------------------------------------

// ActorDied
impl<T: std::fmt::Debug> std::fmt::Display for DidntArrive<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ActorDied")
    }
}
impl<T: std::fmt::Debug> std::error::Error for DidntArrive<T> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

// SendRecvError
impl<T: std::fmt::Debug> std::fmt::Display for ReqRecvError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DidntArrive(arg0) => f.debug_tuple("ActorDied").field(arg0).finish(),
            Self::NoReply => write!(f, "ActorDiedAfterSending"),
        }
    }
}
impl<T: std::fmt::Debug> std::error::Error for ReqRecvError<T> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

// ActorDiedAfterSending
impl std::fmt::Display for NoReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ActorDiedAfterSending")
    }
}
impl std::error::Error for NoReply {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

// TryRecvError
impl std::fmt::Display for TryRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoReplyYet => write!(f, "NoReplyYet"),
            Self::NoReply => write!(f, "ActorDiedAfterSending"),
        }
    }
}
impl std::error::Error for TryRecvError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

//-------------------------------------
// From implementations
//-------------------------------------

use crate::distributed::registry::RegistryGetError;

impl<T> From<NoReply> for ReqRecvError<T> {
    fn from(_: NoReply) -> Self {
        ReqRecvError::NoReply
    }
}

impl<T> From<DidntArrive<T>> for ReqRecvError<T> {
    fn from(e: DidntArrive<T>) -> Self {
        ReqRecvError::DidntArrive(e.0)
    }
}