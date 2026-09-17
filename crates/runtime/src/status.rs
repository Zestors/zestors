use super::*;

/// Represents the status of an actor in its lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Copy)]
pub enum ActorStatus {
    /// The actor is not running. This is either the initial state of the actor, or the actor has exited. It does not accept messages or signals in this state.
    Exited(ExitStatus),

    /// The actor is in the process of initializing. Once the actor calls [`Inbox::recv_event`] (or a related receiving method) for the first time, it will transition to the [`ActorStatus::Running`] state. It accepts messages and signals in this state, but they will not be processed until that transition happens.
    Initializing,

    /// The actor is running and processing messages.
    Running,

    /// The actor is suspended and will not process messages until it is resumed.
    Suspended,

    /// The actor is in the process of shutting down. It will not accept new messages, but will finish processing any messages that are already in the queue.
    Exiting,
}

impl ActorStatus {
    /// Returns `true` if the actor is shutting down or has already exited
    /// (`Exiting` or `Exited`).
    pub fn should_exit(&self) -> bool {
        matches!(self, ActorStatus::Exiting | ActorStatus::Exited(_))
    }

    /// Returns `true` if a message or signal sent now would be accepted
    /// (`Initializing`, `Running`, or `Suspended`) rather than rejected as
    /// closed.
    pub fn accepts_messages(&self) -> bool {
        matches!(
            self,
            ActorStatus::Initializing | ActorStatus::Running | ActorStatus::Suspended
        )
    }

    /// Returns `true` if the status is [`ActorStatus::Running`].
    pub fn is_running(&self) -> bool {
        matches!(self, ActorStatus::Running)
    }

    /// Returns `true` if the status is [`ActorStatus::Suspended`].
    pub fn is_suspended(&self) -> bool {
        matches!(self, ActorStatus::Suspended)
    }

    /// Returns `true` if the status is [`ActorStatus::Exiting`], i.e. the
    /// actor is shutting down but hasn't finished yet.
    pub fn is_exiting(&self) -> bool {
        matches!(self, ActorStatus::Exiting)
    }

    /// Returns `true` if the status is [`ActorStatus::Exited`].
    pub fn is_dead(&self) -> bool {
        matches!(self, ActorStatus::Exited(_))
    }

    /// Returns `true` if the status is [`ActorStatus::Initializing`].
    pub fn is_initializing(&self) -> bool {
        matches!(self, ActorStatus::Initializing)
    }
}

/// The final outcome of an actor's most recently completed run, carried by
/// [`ActorStatus::Exited`] and recorded in the exit history returned by
/// [`ActorOps::snapshot`].
///
/// This mirrors `Result<(), ExitError>` (see [`ExitStatus::from_result`] /
/// [`ExitStatus::into_result`]), flattened into a single [`Copy`]/[`Eq`]/[`Hash`]
/// enum so it can be stored directly in [`ActorStatus`] and cloned into history
/// without going through a `Result`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Copy, thiserror::Error)]
pub enum ExitStatus {
    /// The actor's task returned `Ok(())`.
    #[error("Normal exit")]
    Normal,
    /// The actor's task panicked.
    #[error("Panicked")]
    Panicked,
    /// The actor's task was aborted (e.g. via [`Child::abort`]) before it
    /// could finish.
    #[error("Aborted")]
    Aborted,
    /// The actor's task returned `Err(_)`.
    #[error("Unhandled error")]
    UnhandledError,
}

impl ExitStatus {
    /// Converts a task's result into an [`ExitStatus`], mapping `Ok(())` to
    /// [`ExitStatus::Normal`] and each [`ExitError`] variant to its
    /// corresponding `ExitStatus` variant.
    pub fn from_result(result: Result<(), ExitError>) -> Self {
        match result {
            Ok(_) => ExitStatus::Normal,
            Err(err) => match err {
                ExitError::Panicked => ExitStatus::Panicked,
                ExitError::Aborted => ExitStatus::Aborted,
                ExitError::UnhandledError => ExitStatus::UnhandledError,
            },
        }
    }

    /// The inverse of [`ExitStatus::from_result`].
    pub fn into_result(self) -> Result<(), ExitError> {
        match self {
            ExitStatus::Normal => Ok(()),
            ExitStatus::Panicked => Err(ExitError::Panicked),
            ExitStatus::Aborted => Err(ExitError::Aborted),
            ExitStatus::UnhandledError => Err(ExitError::UnhandledError),
        }
    }

    /// Returns `true` for [`ExitStatus::Normal`].
    pub fn is_normal(&self) -> bool {
        matches!(self, ExitStatus::Normal)
    }

    /// Returns `true` for any variant other than [`ExitStatus::Normal`].
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            ExitStatus::Panicked | ExitStatus::Aborted | ExitStatus::UnhandledError
        )
    }
}

impl From<ExitError> for ExitStatus {
    fn from(err: ExitError) -> Self {
        match err {
            ExitError::Panicked => ExitStatus::Panicked,
            ExitError::Aborted => ExitStatus::Aborted,
            ExitError::UnhandledError => ExitStatus::UnhandledError,
        }
    }
}
