use super::*;
use rootcause::compat::ReportAsError;
use std::fmt::Display;
use thiserror::Error;
use zestors_interface::ReceiptError;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum TryCastError<T> {
    #[error("Channel is closed")]
    Closed(T),

    #[error("Channel is full")]
    Full(T),
}

impl<T> TryCastError<T> {
    pub fn into_inner(self) -> T {
        match self {
            TryCastError::Closed(t) => t,
            TryCastError::Full(t) => t,
        }
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
#[error("Channel is closed")]
pub struct CastError<T>(pub T);

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum CastCheckedError<T> {
    #[error("Channel is closed")]
    Closed(T),

    #[error("Message type not accepted by channel")]
    NotAccepted(T),
}

impl<T> CastCheckedError<T> {
    pub fn into_inner(self) -> T {
        match self {
            CastCheckedError::Closed(t) => t,
            CastCheckedError::NotAccepted(t) => t,
        }
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum TryCastCheckedError<T> {
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

impl<T> TryCastCheckedError<T> {
    pub fn into_inner(self) -> T {
        match self {
            TryCastCheckedError::Closed(t) => t,
            TryCastCheckedError::Full(t) => t,
            TryCastCheckedError::NotAccepted(t) => t,
        }
    }
}

impl<T> From<CastError<T>> for TryCastError<T> {
    fn from(err: CastError<T>) -> Self {
        TryCastError::Closed(err.0)
    }
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum CallError<M> {
    #[error("The channel was closed")]
    Closed(M),

    #[error("No response was received")]
    NoResponse,
}

impl<M> From<CastError<M>> for CallError<M> {
    fn from(err: CastError<M>) -> Self {
        CallError::Closed(err.0)
    }
}

impl<M> From<ReceiptError> for CallError<M> {
    fn from(_err: ReceiptError) -> Self {
        Self::NoResponse
    }
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum CallCheckedError<M> {
    #[error("The channel was closed")]
    Closed(M),

    #[error("The message type was not accepted by the channel")]
    NotAccepted(M),

    #[error("No response was received")]
    NoResponse,
}

impl<M> From<CastCheckedError<M>> for CallCheckedError<M> {
    fn from(err: CastCheckedError<M>) -> Self {
        match err {
            CastCheckedError::Closed(m) => CallCheckedError::Closed(m),
            CastCheckedError::NotAccepted(m) => CallCheckedError::NotAccepted(m),
        }
    }
}

impl<M> From<ReceiptError> for CallCheckedError<M> {
    fn from(_err: ReceiptError) -> Self {
        Self::NoResponse
    }
}

impl<M> From<NotAccepted<M>> for CallCheckedError<M> {
    fn from(err: NotAccepted<M>) -> Self {
        CallCheckedError::NotAccepted(err.0)
    }
}

impl<M> From<CallError<M>> for CallCheckedError<M> {
    fn from(err: CallError<M>) -> Self {
        match err {
            CallError::Closed(m) => CallCheckedError::Closed(m),
            CallError::NoResponse => CallCheckedError::NoResponse,
        }
    }
}

impl<T> From<PushError<T>> for TryCastError<T> {
    fn from(err: PushError<T>) -> Self {
        match err {
            PushError::Closed(t) => TryCastError::Closed(t),
            PushError::Full(t) => TryCastError::Full(t),
        }
    }
}

impl<T> From<CastCheckedError<T>> for TryCastCheckedError<T> {
    fn from(err: CastCheckedError<T>) -> Self {
        match err {
            CastCheckedError::Closed(t) => TryCastCheckedError::Closed(t),
            CastCheckedError::NotAccepted(t) => TryCastCheckedError::NotAccepted(t),
        }
    }
}

impl<T> From<PushError<T>> for TryCastCheckedError<T> {
    fn from(err: PushError<T>) -> Self {
        match err {
            PushError::Closed(t) => TryCastCheckedError::Closed(t),
            PushError::Full(t) => TryCastCheckedError::Full(t),
        }
    }
}

impl<T> From<TryCastError<T>> for TryCastCheckedError<T> {
    fn from(err: TryCastError<T>) -> Self {
        match err {
            TryCastError::Closed(t) => TryCastCheckedError::Closed(t),
            TryCastError::Full(t) => TryCastCheckedError::Full(t),
        }
    }
}

impl<T> From<NotAccepted<T>> for TryCastCheckedError<T> {
    fn from(err: NotAccepted<T>) -> Self {
        TryCastCheckedError::NotAccepted(err.0)
    }
}

impl<T> From<NotAccepted<T>> for CastCheckedError<T> {
    fn from(err: NotAccepted<T>) -> Self {
        CastCheckedError::NotAccepted(err.0)
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
