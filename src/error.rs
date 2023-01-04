pub use tiny_actor::error::{
    ExitError, HaltedError, RecvError, SendError, SpawnError, TryRecvError, TrySendError,
    TrySpawnError,
};

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
