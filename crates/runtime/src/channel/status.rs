use super::*;
use std::fmt::Display;

/// Represents the status of an actor in it's lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Copy)]
pub enum ActorStatus {
    /// The actor is not running. This is either the initial state of the actor, or the actor has exited. It is does not accept messages or signals in this state.
    Exited(ExitStatus),

    /// The actor is in the process of initializing. Once the actor calls [`Inbox::next`] for the first time, it will transition to the [`ActorStatus::Running`] state. It accepts messages and signals in this state, but will not process messages until
    Initializing,

    /// The actor is running and processing messages.
    Running,

    /// The actor is suspended and will not process messages until it is resumed.
    Suspended,

    /// The actor is in the process of shutting down. It will not accept new messages, but will finish processing any messages that are already in the queue.
    Exiting,
}

impl ActorStatus {
    pub fn should_exit(&self) -> bool {
        matches!(self, ActorStatus::Exiting | ActorStatus::Exited(_))
    }

    pub fn accepts_messages(&self) -> bool {
        matches!(
            self,
            ActorStatus::Initializing | ActorStatus::Running | ActorStatus::Suspended
        )
    }

    pub fn is_running(&self) -> bool {
        matches!(self, ActorStatus::Running)
    }

    pub fn is_suspended(&self) -> bool {
        matches!(self, ActorStatus::Suspended)
    }

    pub fn is_shutting_down(&self) -> bool {
        matches!(self, ActorStatus::Exiting)
    }

    pub fn is_dead(&self) -> bool {
        matches!(self, ActorStatus::Exited(_))
    }

    pub fn is_initializing(&self) -> bool {
        matches!(self, ActorStatus::Initializing)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Copy, thiserror::Error)]
pub enum ExitStatus {
    #[error("Normal exit")]
    Normal,
    #[error("Panicked")]
    Panicked,
    #[error("Aborted")]
    Aborted,
    #[error("Unhandled error")]
    UnhandledError,
}

impl ExitStatus {
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

    pub fn into_result(self) -> Result<(), ExitError> {
        match self {
            ExitStatus::Normal => Ok(()),
            ExitStatus::Panicked => Err(ExitError::Panicked),
            ExitStatus::Aborted => Err(ExitError::Aborted),
            ExitStatus::UnhandledError => Err(ExitError::UnhandledError),
        }
    }

    pub fn is_normal(&self) -> bool {
        matches!(self, ExitStatus::Normal)
    }

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
