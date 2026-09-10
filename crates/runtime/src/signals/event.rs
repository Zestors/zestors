use super::*;

#[derive(Debug)]
pub enum Event<M> {
    Signal(Signal),
    Message(M),
}

/// A signal that can be sent to an actor to control its behavior. Signals take
/// precedence over messages, and are processed before any messages in the actor's queue.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, Serialize, Deserialize)]
pub enum Signal {
    Shutdown,
    Suspend,
    Resume,
}

impl Signal {
    pub fn is_shutdown(&self) -> bool {
        matches!(self, Signal::Shutdown)
    }

    pub fn is_resume(&self) -> bool {
        matches!(self, Signal::Resume)
    }

    pub fn is_suspend(&self) -> bool {
        matches!(self, Signal::Suspend)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RestartMode {
    Always,
    OnError,
    Never,
}

impl Default for RestartMode {
    fn default() -> Self {
        RestartMode::OnError
    }
}

impl RestartMode {
    pub fn should_restart(&self, exit: &ExitStatus) -> bool {
        match (self, exit) {
            (RestartMode::Always, _) => true,
            (RestartMode::Never, _) => false,
            (
                RestartMode::OnError,
                ExitStatus::Panicked | ExitStatus::Aborted | ExitStatus::UnhandledError,
            ) => true,
            (RestartMode::OnError, ExitStatus::Normal) => false,
        }
    }
}
