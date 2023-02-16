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
pub enum SendRecvError<M> {
    NoReply,
    Closed(M),
}

/// Error returned when using the [IntoRecv] trait.
///
/// This error combines failures in sending and receiving.
#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum TrySendRecvError<M> {
    NoReply,
    Closed(M),
    Full(M),
}
