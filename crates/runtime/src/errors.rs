use super::*;
use rootcause::compat::ReportAsError;
use std::fmt::Display;
use thiserror::Error;
use zestors_interface::ReceiptError;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum TrySendError<T> {
    #[error("Channel is closed")]
    Closed(T),

    #[error("Channel is full")]
    Full(T),
}

impl<T> TrySendError<T> {
    pub fn into_inner(self) -> T {
        match self {
            TrySendError::Closed(t) => t,
            TrySendError::Full(t) => t,
        }
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
#[error("Channel is closed")]
pub struct SendError<T>(pub T);

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum SendCheckedError<T> {
    #[error("Channel is closed")]
    Closed(T),

    #[error("Message type not accepted by channel")]
    NotAccepted(T),
}

impl<T> SendCheckedError<T> {
    pub fn into_inner(self) -> T {
        match self {
            SendCheckedError::Closed(t) => t,
            SendCheckedError::NotAccepted(t) => t,
        }
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum TrySendCheckedError<T> {
    #[error("Channel is closed")]
    Closed(T),

    #[error("Channel is full")]
    Full(T),

    #[error("Message type not accepted by channel")]
    NotAccepted(T),
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
#[error("Message type not accepted by channel")]
pub struct NotAccepted<T>(pub T);

impl<T> TrySendCheckedError<T> {
    pub fn into_inner(self) -> T {
        match self {
            TrySendCheckedError::Closed(t) => t,
            TrySendCheckedError::Full(t) => t,
            TrySendCheckedError::NotAccepted(t) => t,
        }
    }
}

impl<T> From<SendError<T>> for TrySendError<T> {
    fn from(err: SendError<T>) -> Self {
        TrySendError::Closed(err.0)
    }
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum RequestError<M> {
    #[error("The channel was closed")]
    Closed(M),

    #[error("No response was received")]
    NoResponse,
}

impl<M> From<SendError<M>> for RequestError<M> {
    fn from(err: SendError<M>) -> Self {
        RequestError::Closed(err.0)
    }
}

impl<M> From<ReceiptError> for RequestError<M> {
    fn from(_err: ReceiptError) -> Self {
        Self::NoResponse
    }
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum RequestCheckedError<M> {
    #[error("The channel was closed")]
    Closed(M),

    #[error("The message type was not accepted by the channel")]
    NotAccepted(M),

    #[error("No response was received")]
    NoResponse,
}

impl<M> From<SendCheckedError<M>> for RequestCheckedError<M> {
    fn from(err: SendCheckedError<M>) -> Self {
        match err {
            SendCheckedError::Closed(m) => RequestCheckedError::Closed(m),
            SendCheckedError::NotAccepted(m) => RequestCheckedError::NotAccepted(m),
        }
    }
}

impl<M> From<ReceiptError> for RequestCheckedError<M> {
    fn from(_err: ReceiptError) -> Self {
        Self::NoResponse
    }
}

impl<M> From<NotAccepted<M>> for RequestCheckedError<M> {
    fn from(err: NotAccepted<M>) -> Self {
        RequestCheckedError::NotAccepted(err.0)
    }
}

impl<M> From<RequestError<M>> for RequestCheckedError<M> {
    fn from(err: RequestError<M>) -> Self {
        match err {
            RequestError::Closed(m) => RequestCheckedError::Closed(m),
            RequestError::NoResponse => RequestCheckedError::NoResponse,
        }
    }
}

impl<T> From<PushError<T>> for TrySendError<T> {
    fn from(err: PushError<T>) -> Self {
        match err {
            PushError::Closed(t) => TrySendError::Closed(t),
            PushError::Full(t) => TrySendError::Full(t),
        }
    }
}

impl<T> From<SendCheckedError<T>> for TrySendCheckedError<T> {
    fn from(err: SendCheckedError<T>) -> Self {
        match err {
            SendCheckedError::Closed(t) => TrySendCheckedError::Closed(t),
            SendCheckedError::NotAccepted(t) => TrySendCheckedError::NotAccepted(t),
        }
    }
}

impl<T> From<PushError<T>> for TrySendCheckedError<T> {
    fn from(err: PushError<T>) -> Self {
        match err {
            PushError::Closed(t) => TrySendCheckedError::Closed(t),
            PushError::Full(t) => TrySendCheckedError::Full(t),
        }
    }
}

impl<T> From<TrySendError<T>> for TrySendCheckedError<T> {
    fn from(err: TrySendError<T>) -> Self {
        match err {
            TrySendError::Closed(t) => TrySendCheckedError::Closed(t),
            TrySendError::Full(t) => TrySendCheckedError::Full(t),
        }
    }
}

impl<T> From<NotAccepted<T>> for TrySendCheckedError<T> {
    fn from(err: NotAccepted<T>) -> Self {
        TrySendCheckedError::NotAccepted(err.0)
    }
}

impl<T> From<NotAccepted<T>> for SendCheckedError<T> {
    fn from(err: NotAccepted<T>) -> Self {
        SendCheckedError::NotAccepted(err.0)
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Copy)]
pub enum ExitError {
    #[error("Actor panicked")]
    Panicked,

    #[error("Actor was aborted")]
    Aborted,

    #[error("Actor exited with error")]
    UnhandledError,
}

#[derive(Debug, Error)]
#[error("Failed to spawn process: {0}")]
pub enum StartOnError {
    #[error("There is already an active process running on this channel.")]
    ConcurrentInbox,

    #[error("Failed to instantiate actor from blueprint: {0}")]
    Instantiation(#[source] ReportAsError),
}

impl From<ConcurrentInboxError> for StartOnError {
    fn from(_: ConcurrentInboxError) -> Self {
        StartOnError::ConcurrentInbox
    }
}

#[derive(Debug, Error)]
#[error("There is already an active process running on this channel.")]
pub struct ConcurrentInboxError;

#[derive(thiserror::Error, Debug)]
pub enum JoinError {
    /// The task panicked.
    #[error("task panicked")]
    Panic,

    /// The task was aborted.
    #[error("task was aborted / cancelled")]
    Aborted,

    /// The actor exited with an unhandled error.
    #[error("task returned an error: {0}")]
    UnhandledError(Report),
}

impl From<tokio::task::JoinError> for JoinError {
    fn from(err: tokio::task::JoinError) -> Self {
        if err.is_cancelled() {
            JoinError::Aborted
        } else if err.is_panic() {
            JoinError::Panic
        } else {
            unreachable!("JoinError is neither cancelled nor panicked: {:?}", err)
        }
    }
}

impl From<ShutdownAbortError> for JoinError {
    fn from(err: ShutdownAbortError) -> Self {
        err.error
    }
}

impl JoinError {
    pub(super) fn into_join_abort(self, aborted: bool, timeout: Duration) -> ShutdownAbortError {
        ShutdownAbortError {
            aborted,
            timeout,
            error: self,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub struct ShutdownAbortError {
    pub aborted: bool,
    pub timeout: Duration,
    pub error: JoinError,
}

impl Display for ShutdownAbortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.aborted {
            write!(
                f,
                "Child was aborted due to timeout of {:?}. Error: {}",
                self.timeout, self.error
            )
        } else {
            write!(f, "Child exited with error: {}", self.error)
        }
    }
}

#[derive(Debug, thiserror::Error, Clone)]
#[error("Duplicate PID: {pid} already exists in the registry")]
pub struct DuplicatePidError {
    pub pid: Pid,
}

#[derive(Debug, thiserror::Error, Clone)]
#[error("The operation was cancelled")]
pub struct Cancelled;
