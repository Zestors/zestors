/// Error returned when trying to add a process to an actor.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AddProcessError {
    #[error("The actor has exited so no more processes may be added.")]
    ActorHasExited,
    #[error("The actor has a channel that does not support multiple inboxes.")]
    SingleProcessOnly,
}
