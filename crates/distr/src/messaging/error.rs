use super::{DecodeError, EncodeError};

/// Why the node a message was sent to didn't deliver it, or its reply.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RemoteError {
    /// The node doesn't have this message type registered.
    #[error("The node doesn't know this message type")]
    UnknownMessage,
    /// No actor with this name is running on the node.
    #[error("No such actor on the node")]
    NoSuchActor,
    /// The actor doesn't accept this message type.
    #[error("The actor doesn't accept this message")]
    NotAccepted,
    /// The actor is not taking messages any more.
    #[error("The actor is closed")]
    Closed,
    /// Too much is waiting to be delivered to the actor already.
    #[error("The actor has too much waiting for it")]
    Overloaded,
    /// The actor dropped the request without replying to it.
    #[error("The actor did not reply")]
    NoReply,
    /// The node could not decode the message.
    #[error("The node could not decode the message: {0}")]
    Decode(String),
    /// The node could not encode the reply.
    #[error("The node could not encode the reply: {0}")]
    Encode(String),
    /// The reply is too large to send back.
    #[error("The reply is too large to send")]
    TooLarge,
}

/// A message couldn't be sent to a remote actor. It is given back.
#[derive(Debug, thiserror::Error)]
pub enum RemoteCastError<M> {
    /// The node hasn't started yet, or has stopped.
    #[error("The node is not running")]
    NotRunning(M),
    /// The node the actor is on isn't part of the cluster.
    #[error("The node is not part of the cluster")]
    NotAMember(M),
    /// The node can't be connected to right now.
    #[error("The node can't be reached")]
    Unreachable(M),
    /// The encoded message is larger than can be sent.
    #[error("The message is too large: {size} bytes, at most {max} can be sent")]
    TooLarge { msg: M, size: usize, max: usize },
    /// The message couldn't be encoded.
    #[error("Failed to encode the message: {error}")]
    Encode { msg: M, error: EncodeError },
    /// The messages waiting to be sent to the node are too many; see
    /// [`RemoteAccepts::try_cast`](super::RemoteAccepts::try_cast).
    #[error("Too many messages are waiting to be sent to the node")]
    Full(M),
}

impl<M> RemoteCastError<M> {
    /// The message that was not sent.
    pub fn into_inner(self) -> M {
        match self {
            RemoteCastError::NotRunning(msg)
            | RemoteCastError::NotAMember(msg)
            | RemoteCastError::Unreachable(msg)
            | RemoteCastError::Full(msg)
            | RemoteCastError::TooLarge { msg, .. }
            | RemoteCastError::Encode { msg, .. } => msg,
        }
    }
}

/// A message was sent to a remote actor, but no reply came.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RemoteReplyError {
    /// The node answered with an error.
    #[error("The node could not deliver the message: {0}")]
    Remote(#[from] RemoteError),
    /// The connection to the node was lost, or the node left, before the reply
    /// came. The message may or may not have been delivered.
    #[error("The node was lost before it replied")]
    Disconnected,
    /// No reply came in time.
    #[error("The node did not reply in time")]
    Timeout,
    /// The reply couldn't be decoded.
    #[error("Failed to decode the reply: {0}")]
    Decode(#[from] DecodeError),
}

/// A call to a remote actor failed.
#[derive(Debug, thiserror::Error)]
pub enum RemoteCallError<M> {
    /// The message was not sent, and is given back.
    #[error(transparent)]
    NotSent(#[from] RemoteCastError<M>),
    /// The message was sent, but no reply came.
    #[error(transparent)]
    Reply(RemoteReplyError),
}
