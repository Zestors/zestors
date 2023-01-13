mod accepts;
mod address;
pub use accepts::*;
pub use address::*;
use concurrent_queue::PushError;
use thiserror::Error;

//------------------------------------------------------------------------------------------------
//  Errors
//------------------------------------------------------------------------------------------------

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum TrySendUncheckedError<M> {
    Full(M),
    Closed(M),
    NotAccepted(M),
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum SendUncheckedError<M> {
    Closed(M),
    NotAccepted(M),
}

/// An error returned when trying to send a message into a channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum TrySendError<M> {
    /// The channel has been closed, and no longer accepts new messages.
    #[error("Couldn't send message because Channel is closed")]
    Closed(M),
    /// The channel is full.
    #[error("Couldn't send message because Channel is full")]
    Full(M),
}

impl<M> From<PushError<M>> for TrySendError<M> {
    fn from(e: PushError<M>) -> Self {
        match e {
            PushError::Full(msg) => Self::Full(msg),
            PushError::Closed(msg) => Self::Closed(msg),
        }
    }
}

/// Error returned when sending a message into a channel.
///
/// The channel has been closed, and no longer accepts new messages.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub struct SendError<M>(pub M);
