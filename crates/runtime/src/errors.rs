//! Error types returned by this crate.
//!
//! The sending errors form a small grid, along two axes:
//! - *static* ([`Accepts`]) vs. *dynamic* (`Dyn` suffix, [`ActorOps::cast_dyn`]
//!   and friends) - dynamic sends can additionally fail with `NotAccepted`,
//!   since acceptance is checked at runtime rather than compile time.
//! - *fire-and-forget send* ([`TryCastError`]/[`CastError`]/[`CastDynError`]/
//!   [`TryCastDynError`]) vs. *send-and-await-a-reply*
//!   ([`CallError`]/[`CallDynError`]), which additionally fail with
//!   `NoResponse` if a reply never arrives.
//!
//! Each of those carries the message back on failure (via [`TryCastError::into_inner`]
//! and friends), so a failed send never silently drops it.

use super::*;
use std::fmt::Display;
use thiserror::Error;
use zestors_interface::ReceiptError;

/// Returned by [`Accepts::try_cast`]/[`Accepts::try_cast_with`].
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum TryCastError<T> {
    /// The channel is closed: the actor is [`ActorStatus::Exited`], or
    /// [`ActorStatus::Exiting`] without [`CallOptions::ignore_exiting`].
    #[error("Channel is closed")]
    Closed(T),

    /// The channel is under backpressure and [`CallOptions::ignore_backpressure`]
    /// wasn't set; see [`Accepts::try_cast_with`].
    #[error("Channel is full")]
    Full(T),
}

impl<T> TryCastError<T> {
    /// Extracts the message back out, regardless of which case this was.
    pub fn into_inner(self) -> T {
        match self {
            TryCastError::Closed(t) => t,
            TryCastError::Full(t) => t,
        }
    }

    pub(crate) fn into_cast_error_dbg_assert(self) -> CastError<T> {
        match self {
            TryCastError::Closed(t) => CastError(t),
            TryCastError::Full(t) => {
                debug_assert!(false, "Cannot convert TryCastError::Full into CastError");
                tracing::error!("Cannot convert TryCastError::Full into CastError");
                CastError(t)
            }
        }
    }
}

/// Returned by [`Accepts::cast`]/[`Accepts::cast_with`]. Unlike
/// [`TryCastError`], there's no `Full` case: `cast` waits out backpressure
/// instead of failing because of it.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
#[error("Channel is closed")]
pub struct CastError<T>(pub T);

/// Returned by [`ActorOps::cast_dyn`]/[`ActorOps::cast_dyn_with`].
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum CastDynError<T> {
    /// See [`TryCastError::Closed`].
    #[error("Channel is closed")]
    Closed(T),

    /// The channel's [`Interface`] doesn't have a variant for this message
    /// type.
    #[error("Message type not accepted by channel")]
    NotAccepted(T),
}

impl<T> CastDynError<T> {
    /// Extracts the message back out, regardless of which case this was.
    pub fn into_inner(self) -> T {
        match self {
            CastDynError::Closed(t) => t,
            CastDynError::NotAccepted(t) => t,
        }
    }
}

/// Returned by [`ActorOps::try_cast_dyn`]/[`ActorOps::try_cast_dyn_with`].
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
pub enum TryCastDynError<T> {
    /// See [`TryCastError::Closed`].
    #[error("Channel is closed")]
    Closed(T),

    /// See [`TryCastError::Full`].
    #[error("Channel is full")]
    Full(T),

    /// See [`CastDynError::NotAccepted`].
    #[error("Message type not accepted by channel")]
    NotAccepted(T),
}

/// The channel's [`Interface`] doesn't have a variant for this message type.
/// A standalone building block, converted into the `NotAccepted` case of the
/// larger dynamic-send errors above.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash)]
#[error("Message type not accepted by channel")]
pub struct NotAccepted<T>(pub T);

impl<T> TryCastDynError<T> {
    /// Extracts the message back out, regardless of which case this was.
    pub fn into_inner(self) -> T {
        match self {
            TryCastDynError::Closed(t) => t,
            TryCastDynError::Full(t) => t,
            TryCastDynError::NotAccepted(t) => t,
        }
    }

    pub(crate) fn into_cast_error_dbg_assert(self) -> CastDynError<T> {
        match self {
            TryCastDynError::Closed(t) => CastDynError::Closed(t),
            TryCastDynError::NotAccepted(t) => CastDynError::NotAccepted(t),
            TryCastDynError::Full(t) => {
                debug_assert!(
                    false,
                    "Cannot convert TryCastDynError::Full into CastDynError"
                );
                tracing::error!("Cannot convert TryCastDynError::Full into CastDynError");
                CastDynError::NotAccepted(t)
            }
        }
    }
}

impl<T> From<CastError<T>> for TryCastError<T> {
    fn from(err: CastError<T>) -> Self {
        TryCastError::Closed(err.0)
    }
}

/// Returned by [`Accepts::call`]/[`Accepts::call_with`].
#[derive(Debug, thiserror::Error, Clone)]
pub enum CallError<M> {
    /// The channel was already closed at the time of sending; see
    /// [`CastError`].
    #[error("The channel was closed")]
    Closed(M),

    /// The message was sent, but no reply ever arrived - for example
    /// because the actor exited, or dropped the request, before replying.
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

/// Returned by [`ActorOps::call_dyn`]/[`ActorOps::call_dyn_with`].
#[derive(Debug, thiserror::Error, Clone)]
pub enum CallDynError<M> {
    /// See [`CallError::Closed`].
    #[error("The channel was closed")]
    Closed(M),

    /// See [`CastDynError::NotAccepted`].
    #[error("The message type was not accepted by the channel")]
    NotAccepted(M),

    /// See [`CallError::NoResponse`].
    #[error("No response was received")]
    NoResponse,
}

impl<M> From<CastDynError<M>> for CallDynError<M> {
    fn from(err: CastDynError<M>) -> Self {
        match err {
            CastDynError::Closed(m) => CallDynError::Closed(m),
            CastDynError::NotAccepted(m) => CallDynError::NotAccepted(m),
        }
    }
}

impl<M> From<ReceiptError> for CallDynError<M> {
    fn from(_err: ReceiptError) -> Self {
        Self::NoResponse
    }
}

impl<M> From<NotAccepted<M>> for CallDynError<M> {
    fn from(err: NotAccepted<M>) -> Self {
        CallDynError::NotAccepted(err.0)
    }
}

impl<M> From<CallError<M>> for CallDynError<M> {
    fn from(err: CallError<M>) -> Self {
        match err {
            CallError::Closed(m) => CallDynError::Closed(m),
            CallError::NoResponse => CallDynError::NoResponse,
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

impl<T> From<CastDynError<T>> for TryCastDynError<T> {
    fn from(err: CastDynError<T>) -> Self {
        match err {
            CastDynError::Closed(t) => TryCastDynError::Closed(t),
            CastDynError::NotAccepted(t) => TryCastDynError::NotAccepted(t),
        }
    }
}

impl<T> From<PushError<T>> for TryCastDynError<T> {
    fn from(err: PushError<T>) -> Self {
        match err {
            PushError::Closed(t) => TryCastDynError::Closed(t),
            PushError::Full(t) => TryCastDynError::Full(t),
        }
    }
}

impl<T> From<TryCastError<T>> for TryCastDynError<T> {
    fn from(err: TryCastError<T>) -> Self {
        match err {
            TryCastError::Closed(t) => TryCastDynError::Closed(t),
            TryCastError::Full(t) => TryCastDynError::Full(t),
        }
    }
}

impl<T> From<NotAccepted<T>> for TryCastDynError<T> {
    fn from(err: NotAccepted<T>) -> Self {
        TryCastDynError::NotAccepted(err.0)
    }
}

impl<T> From<NotAccepted<T>> for CastDynError<T> {
    fn from(err: NotAccepted<T>) -> Self {
        CastDynError::NotAccepted(err.0)
    }
}

/// The non-normal ways an actor's task can end, carried by
/// [`ActorStatus::Exited`]/[`ExitStatus`]. Unlike [`ExitStatus`], this has no
/// variant for a normal `Ok(())` return - it's the `Result::Err` side of
/// that same outcome (see [`ExitStatus::from_result`]).
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Copy)]
pub enum ExitError {
    /// The actor's task panicked.
    #[error("Actor panicked")]
    Panicked,

    /// The actor's task was aborted (e.g. via [`Child::abort`]) before it
    /// could finish.
    #[error("Actor was aborted")]
    Aborted,

    /// The actor's task returned `Err(_)`.
    #[error("Actor exited with error")]
    UnhandledError,
}

/// Returned by [`StrongAddress::spawn`]/[`StrongAddress::spawn_task`]: a
/// process is already running on this channel.
#[derive(Debug, Error)]
#[error("There is already an active process running on this channel.")]
pub struct ConcurrentInboxError;

/// The `Err` side of awaiting a [`Child`] (it implements `Future<Output =
/// Result<E, JoinError>>`) - covers a panic, an abort, or the actor's own
/// task returning `Err`. See [`ExitError`] for the same three outcomes,
/// tracked instead on the channel's [`ActorStatus`].
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

/// Returned by [`Child::shutdown_abort`]: the child was signaled to shut
/// down, but either didn't finish within the given timeout (`aborted:
/// true`) or finished anyway with a [`JoinError`] of its own (`aborted:
/// false`).
#[derive(thiserror::Error, Debug)]
pub struct ShutdownAbortError {
    /// Whether the timeout was actually hit and the child had to be aborted,
    /// as opposed to exiting (still unsuccessfully) on its own.
    pub aborted: bool,
    /// The timeout that was given to [`Child::shutdown_abort`].
    pub timeout: Duration,
    /// The underlying [`JoinError`] - `Aborted` if `aborted` is `true`,
    /// otherwise whatever the child itself ended with.
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

/// Returned by [`spawn`]/[`spawn_task`]/[`StrongAddress::create`]: `pid` is
/// already registered.
#[derive(Debug, thiserror::Error, Clone)]
#[error("Duplicate PID: {pid} already exists in the registry")]
pub struct DuplicatePidError {
    pub pid: Pid,
}

/// Returned by [`Inbox::run_until_shutdown`]/[`TaskBox::run_until_shutdown`]
/// when a [`Signal::Shutdown`] cancels the future they were driving.
#[derive(Debug, thiserror::Error, Clone)]
#[error("The operation was cancelled")]
pub struct Cancelled;
