
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TryAddProcessError {
    #[error("The actor has exited so no more processes may be added.")]
    ActorHasExited,
    #[error("The actor has a channel that does not support multiple inboxes.")]
    SingleInboxOnly,
}