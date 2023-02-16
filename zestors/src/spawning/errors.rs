use thiserror::Error;

/// An error returned when trying to spawn more processes onto a channel
#[derive(Clone, PartialEq, Eq, Hash, Error)]
pub enum TrySpawnError<T> {
    /// The actor has exited.
    #[error("Couldn't spawn process because the actor has exited")]
    Exited(T),
    /// The spawned inbox does not have the correct type
    #[error("Couldn't spawn process because the given inbox-type is incorrect")]
    IncorrectType(T),
    #[error("This channel does not allow spawning of multiple processes")]
    NotPermitted(T),
}

impl<T> std::fmt::Debug for TrySpawnError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exited(_) => f.debug_tuple("Exited").finish(),
            Self::IncorrectType(_) => f.debug_tuple("IncorrectType").finish(),
            Self::NotPermitted(_) => f.debug_tuple("NotPermitted").finish(),
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
