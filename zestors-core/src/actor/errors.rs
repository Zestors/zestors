//! This module contains all errors

use concurrent_queue::PushError;
use std::any::Any;
use thiserror::Error;

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

/// Error returned when `Stream`ing an [Inbox].
///
/// Process has been halted and should now exit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
#[error("Process has been halted")]
pub struct HaltedError;

/// Error returned when receiving a message from an inbox.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum RecvError {
    /// Process has been halted and should now exit.
    #[error("Couldn't receive because the process has been halted")]
    Halted,
    /// Channel has been closed, and contains no more messages. It is impossible for new
    /// messages to be sent to the channel.
    #[error("Couldn't receive becuase the channel is closed and empty")]
    ClosedAndEmpty,
}

/// Error returned when receiving a message from an inbox.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum TryRecvError {
    /// Process has been halted and should now exit.
    #[error("Couldn't receive because the process has been halted")]
    Halted,
    /// The channel is empty, but is not yet closed. New messges may arrive
    #[error("Couldn't receive because the channel is empty")]
    Empty,
    /// Channel has been closed, and contains no more messages. It is impossible for new
    /// messages to be sent to the channel.
    #[error("Couldn't receive becuase the channel is closed and empty")]
    ClosedAndEmpty,
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

/// An error returned when trying to spawn more processes onto a channel
#[derive(Clone, PartialEq, Eq, Hash, Error)]
pub enum TrySpawnError<T> {
    /// The actor has exited.
    #[error("Couldn't spawn process because the actor has exited")]
    Exited(T),
    /// The spawned inbox does not have the correct type
    #[error("Couldn't spawn process because the given inbox-type is incorrect")]
    IncorrectType(T),
}

impl<T> std::fmt::Debug for TrySpawnError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exited(_) => f.debug_tuple("Exited").finish(),
            Self::IncorrectType(_) => f.debug_tuple("IncorrectType").finish(),
        }
    }
}

/// An error returned when spawning more processes onto a channel.
///
/// The actor has exited.
#[derive(Clone, PartialEq, Eq, Hash, Error)]
#[error("Couldn't spawn process because the channel has exited")]
pub struct SpawnError<T>(pub T);

impl<T> std::fmt::Debug for SpawnError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SpawnError").finish()
    }
}

/// An error returned from an exiting process.
#[derive(Debug, Error)]
pub enum ExitError {
    /// Process panicked.
    #[error("Process has exited because of a panic")]
    Panic(Box<dyn Any + Send>),
    /// Process  was aborted.
    #[error("Process has exited because it was aborted")]
    Abort,
}

impl ExitError {
    /// Whether the error is a panic.
    pub fn is_panic(&self) -> bool {
        match self {
            ExitError::Panic(_) => true,
            ExitError::Abort => false,
        }
    }

    /// Whether the error is an abort.
    pub fn is_abort(&self) -> bool {
        match self {
            ExitError::Panic(_) => false,
            ExitError::Abort => true,
        }
    }
}

impl From<tokio::task::JoinError> for ExitError {
    fn from(e: tokio::task::JoinError) -> Self {
        match e.try_into_panic() {
            Ok(panic) => ExitError::Panic(panic),
            Err(_) => ExitError::Abort,
        }
    }
}
