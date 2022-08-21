use tokio::sync::oneshot;

//------------------------------------------------------------------------------------------------
//  Exports
//------------------------------------------------------------------------------------------------

pub use zestors_core::error::{
    ExitError, HaltedError, RecvError, SendError, SendUncheckedError, SpawnError, TryRecvError,
    TrySendError, TrySendUncheckedError, TrySpawnError,
};

//------------------------------------------------------------------------------------------------
//  New errors
//------------------------------------------------------------------------------------------------

/// Error returned when sending a message using a [Tx].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to send to Tx because it is closed.")]
pub struct TxError<M>(pub M);

/// Error returned when receiving a message using an [Rx].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
#[error("Failed to receive from Rx because it is closed.")]
pub struct RxError;

impl From<oneshot::error::RecvError> for RxError {
    fn from(_: oneshot::error::RecvError) -> Self {
        Self
    }
}

/// Error returned when trying to receive a message using an [Rx].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, thiserror::Error)]
pub enum TryRxError {
    #[error("Closed")]
    Closed,
    #[error("Empty")]
    Empty,
}

impl From<oneshot::error::TryRecvError> for TryRxError {
    fn from(e: oneshot::error::TryRecvError) -> Self {
        match e {
            oneshot::error::TryRecvError::Empty => Self::Empty,
            oneshot::error::TryRecvError::Closed => Self::Closed,
        }
    }
}
