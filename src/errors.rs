/// Error returned when trying to send a [Msg] or [Req] to an [Actor].
///
/// Sending failed because the [Actor] died before the [Msg] or [Req] could be sent.
#[derive(Debug)]
pub struct DidntArrive<T>(pub T);

#[derive(Debug)]
pub struct NoReply<T>(pub T);

/// Error returned when sending a [Msg] or [Req] to an [Actor] and then
/// trying to wait for the [Reply].
#[derive(Debug)]
pub enum SendRecvError<T> {
    /// The [Actor] has died before the [Req] could be sent.
    ActorDied(T),
    /// The [Actor] has died after the [Req] was sent, but before it could reply.
    ActorDiedAfterSending,
}

/// Error returned when trying to send a [Msg] or [Req] to an [Actor].
#[derive(Debug)]
pub enum TrySendError<T> {
    /// The [Actor] has died before the [Msg] or [Req] could be sent.
    ActorDied(T),
    /// The [Actor] has no more space in it's inbox.
    NoSpace(T),
}

/// Error returned when trying to send a [Req] to an [Actor], and then waiting for
/// the [Reply].
#[derive(Debug)]
pub enum TrySendRecvError<T> {
    /// The [Actor] has no more space in it's inbox.
    NoSpace(T),
    /// The [Actor] has died before the [Req] could be sent.
    ActorDied(T),
    /// The [Actor] has died after the [Req] was sent, but before it could reply.
    ActorDiedAfterSending,
}

/// Error returned when waiting for a [Reply] from an [Actor].
///
/// The [Actor] has died after the [Msg] was sent.
#[derive(Debug)]
pub struct ActorDiedAfterSending;

/// Error returned when trying to receive a [Reply] from an [Actor].
#[derive(Debug)]
pub enum TryRecvError {
    /// The [Actor] has died after the [Req] was sent, but before it could reply.
    ActorDiedAfterSending,
    /// The [Actor] has not yet sent back a [Reply], but is still alive.
    NoReplyYet,
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
impl<T: std::fmt::Debug> std::fmt::Display for SendRecvError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActorDied(arg0) => f.debug_tuple("ActorDied").field(arg0).finish(),
            Self::ActorDiedAfterSending => write!(f, "ActorDiedAfterSending"),
        }
    }
}
impl<T: std::fmt::Debug> std::error::Error for SendRecvError<T> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

// TrySendError
impl<T: std::fmt::Debug> std::fmt::Display for TrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActorDied(arg0) => f.debug_tuple("ActorDied").field(arg0).finish(),
            Self::NoSpace(arg0) => f.debug_tuple("NoSpace").field(arg0).finish(),
        }
    }
}
impl<T: std::fmt::Debug> std::error::Error for TrySendError<T> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

// TrySendRecvError
impl<T: std::fmt::Debug> std::fmt::Display for TrySendRecvError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActorDied(arg0) => f.debug_tuple("ActorDied").field(arg0).finish(),
            Self::NoSpace(arg0) => f.debug_tuple("NoSpace").field(arg0).finish(),
            Self::ActorDiedAfterSending => write!(f, "ActorDiedAfterSending"),
        }
    }
}
impl<T: std::fmt::Debug> std::error::Error for TrySendRecvError<T> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

// ActorDiedAfterSending
impl std::fmt::Display for ActorDiedAfterSending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ActorDiedAfterSending")
    }
}
impl std::error::Error for ActorDiedAfterSending {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

// TryRecvError
impl std::fmt::Display for TryRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoReplyYet => write!(f, "NoReplyYet"),
            Self::ActorDiedAfterSending => write!(f, "ActorDiedAfterSending"),
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

impl<T> From<ActorDiedAfterSending> for SendRecvError<T> {
    fn from(_: ActorDiedAfterSending) -> Self {
        SendRecvError::ActorDiedAfterSending
    }
}

impl<T> From<ActorDiedAfterSending> for TrySendRecvError<T> {
    fn from(_: ActorDiedAfterSending) -> Self {
        TrySendRecvError::ActorDiedAfterSending
    }
}
