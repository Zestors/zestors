use thiserror::Error;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum TrySendCheckedError<M> {
    Full(M),
    Closed(M),
    NotAccepted(M),
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum SendCheckedError<M> {
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

/// Error returned when sending a message into a channel.
///
/// The channel has been closed, and no longer accepts new messages.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub struct SendError<M>(pub M);

/// Error returned when using the [IntoRecv] trait.
///
/// This error combines failures in sending and receiving.
#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum RequestError<M, E> {
    NoReply(E),
    Closed(M),
}

/// Error returned when using the [IntoRecv] trait.
///
/// This error combines failures in sending and receiving.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum TryRequestError<M, E> {
    NoReply(E),
    Closed(M),
    Full(M),
}

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

/// This process has been halted and should now exit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
#[error("Process has been halted")]
pub struct Halted;
