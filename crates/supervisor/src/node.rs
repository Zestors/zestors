use crate::_prelude::*;
use futures::future::BoxFuture;
use std::time::Duration;
use zestors_runtime::errors::{JoinError, ShutdownAbortError};
use zestors_supervision::StartOnError;

/// The reason a [`Node`] stopped running.
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    /// The root supervisor failed to start in the first place.
    #[error("Failed to start root supervisor: {0}")]
    StartFailed(#[source] StartOnError),

    /// The root supervisor exited with an error.
    #[error("Root-Supervisor exited with error: {0}")]
    SupervisorExited(#[source] JoinError),

    /// The root supervisor didn't exit within its abort timeout after a
    /// shutdown signal was received.
    #[error("Root-Supervisor failed to exit gracefully within timeout: {0}")]
    ShutdownFailed(#[source] ShutdownAbortError),

    /// A second shutdown signal was received while still waiting for the
    /// root supervisor to exit gracefully from the first one.
    #[error("Forced shutdown: received a second shutdown signal while waiting for graceful exit")]
    ForcedShutdown,
}

impl NodeError {
    /// The process exit code this error should be reported with.
    pub fn exit_code(&self) -> i32 {
        match self {
            NodeError::ForcedShutdown => 130,
            NodeError::StartFailed(_)
            | NodeError::SupervisorExited(_)
            | NodeError::ShutdownFailed(_) => 1,
        }
    }
}

/// Runs a single root [`Supervisor`] as an entire program: starts it, and
/// shuts it down gracefully on a Ctrl+C/SIGTERM — forcing immediate
/// termination if a second signal arrives before it finishes. The node exits
/// whenever the root supervisor does, and is never restarted: if it should
/// be, that is for whatever runs the program.
pub struct Node {
    supervisor_spec: ChildSpec<SupervisorBlueprint>,
    exit_watcher: BoxFuture<'static, ()>,
}

impl Node {
    /// Creates a [`Node`] for the root supervisor described by `spec`.
    pub fn new(spec: ChildSpec<SupervisorBlueprint>) -> Self {
        Self {
            supervisor_spec: spec,
            exit_watcher: Box::pin(wait_for_shutdown_signal()),
        }
    }

    /// Sets a custom exit watcher for the node. The exit watcher is a future
    /// that resolves when the node should begin shutting down.
    ///
    /// By default, it listens for Ctrl+C/SIGTERM signals. This method allows
    /// overriding that behavior with a custom future.
    pub fn with_exit_watcher(
        mut self,
        exit_watcher: impl Future<Output = ()> + Send + 'static,
    ) -> Self {
        self.exit_watcher = Box::pin(exit_watcher);
        self
    }

    /// Starts the root supervisor and runs until the node exits — either
    /// because the supervisor exited, a shutdown signal was handled, or the
    /// supervisor failed (see [`NodeError`]).
    pub async fn run(self) -> Result<(), NodeError> {
        let mut supervisor_child = self.supervisor_spec.start().await.map_err(|err| {
            tracing::error!("Failed to start supervisor: {:?}", err);
            NodeError::StartFailed(err)
        })?;

        tokio::select! {
            exit = &mut supervisor_child => match exit {
                Ok(()) => {
                    tracing::info!("Root-Supervisor exited gracefully. Shutting down node.");
                    Ok(())
                }
                Err(err) => {
                    tracing::error!("Root-Supervisor exited with error: {:?}", err);
                    Err(NodeError::SupervisorExited(err))
                }
            },

            _ = self.exit_watcher => {
                tracing::info!("Received Ctrl+C signal. Shutting down node.");

                let timeout = self.supervisor_spec.cfg().abort_timeout;
                tokio::select! {
                    exit = supervisor_child.shutdown_abort(timeout) => {
                        match exit {
                            Ok(()) => {
                                tracing::info!("Root-Supervisor exited gracefully. Shutting down node.");
                                Ok(())
                            }
                            Err(err) => {
                                tracing::error!("Root-Supervisor failed to exit gracefully within timeout: {:?}", err);
                                tokio::time::sleep(Duration::from_secs(3)).await;
                                Err(NodeError::ShutdownFailed(err))
                            }
                        }
                    }
                    _ = wait_for_shutdown_signal() => {
                        tracing::warn!("Received second shutdown signal. Forcing immediate termination.");
                        Err(NodeError::ForcedShutdown)
                    }
                }
            }
        }
    }

    /// Returns the root supervisor's [`ChildSpec`].
    ///
    /// Signals sent to its address are only accepted once the supervisor is
    /// initializing or running, so wait for that first (see
    /// [`ActorStatus::accepts_messages`](zestors_runtime::ActorStatus::accepts_messages))
    /// before asking the node to shut down with `signal_shutdown`.
    pub fn root_supervisor(&self) -> &ChildSpec<SupervisorBlueprint> {
        &self.supervisor_spec
    }
}

async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C (SIGINT) signal."),
        _ = terminate => tracing::info!("Received SIGTERM signal."),
    }
}
