use super::{DecodeError, EncodeError};
use zestors_runtime::{Name, TypedRegistryError};

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
    /// The actor is on this node and is not taking messages any more.
    #[error("The actor is closed")]
    Closed(M),
    /// The actor is on this node and doesn't accept this message type.
    #[error("The actor doesn't accept this message")]
    NotAccepted(M),
}

impl<M> RemoteCastError<M> {
    /// The message that was not sent.
    pub fn into_inner(self) -> M {
        match self {
            RemoteCastError::NotRunning(msg)
            | RemoteCastError::NotAMember(msg)
            | RemoteCastError::Unreachable(msg)
            | RemoteCastError::Full(msg)
            | RemoteCastError::Closed(msg)
            | RemoteCastError::NotAccepted(msg)
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

/// An operation on a remote actor failed, see
/// [`RemoteActorOps`](super::RemoteActorOps).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RemoteOpError {
    /// The request couldn't be sent.
    #[error(transparent)]
    NotSent(#[from] RemoteCastError<()>),
    /// The request was sent, but no answer came.
    #[error(transparent)]
    Reply(#[from] RemoteReplyError),
    /// The actor is on this node, which can't answer that.
    #[error("Not supported for an actor on this node")]
    Unsupported,
}

/// An address for an actor couldn't be made.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AddressError {
    /// No actor with this name is running on its node.
    #[error("No actor named {0} on its node")]
    NoSuchActor(Name),
    /// The actor doesn't accept what the address is for, or its node hasn't
    /// registered it.
    #[error("The actor named {0} doesn't accept what the address is for")]
    TypeMismatch(Name),
    /// The node of the actor couldn't be asked.
    #[error("Failed to ask the node of the actor: {0}")]
    Remote(#[from] RemoteOpError),
}

impl From<TypedRegistryError> for AddressError {
    fn from(error: TypedRegistryError) -> Self {
        match error {
            TypedRegistryError::NotFound(name) => AddressError::NoSuchActor(name),
            TypedRegistryError::TypeMismatch(name) => AddressError::TypeMismatch(name),
        }
    }
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
