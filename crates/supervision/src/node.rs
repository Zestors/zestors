use crate::_prelude::*;
use std::time::Duration;
use zestors_runtime::errors::{JoinError, ShutdownAbortError, StartOnError};

#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("Failed to start root supervisor: {0}")]
    StartFailed(#[source] StartOnError),

    #[error("Root-Supervisor exited with error: {0}")]
    SupervisorExited(#[source] JoinError),

    #[error("Root-Supervisor failed to exit gracefully within timeout: {0}")]
    ShutdownFailed(#[source] ShutdownAbortError),

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

pub struct Node {
    restart_intensity: RestartIntensity,
    supervisor_spec: ChildSpec<SupervisorBlueprint>,
}

struct NodeActor {
    supervisor_child: Child<(), SupervisorInterface>,
    restart_limiter: RestartLimiter,
    supervisor_spec: ChildSpec<SupervisorBlueprint>,
}

impl Node {
    pub fn new(spec: ChildSpec<SupervisorBlueprint>) -> Self {
        Self {
            restart_intensity: RestartIntensity::restarts(0),
            supervisor_spec: spec,
        }
    }

    pub fn with_restart_intensity(mut self, intensity: RestartIntensity) -> Self {
        self.restart_intensity = intensity;
        self
    }

    pub async fn run(self) -> Result<(), NodeError> {
        let Self {
            restart_intensity,
            supervisor_spec,
        } = self;

        let supervisor_child = supervisor_spec.start().await.map_err(|err| {
            tracing::error!("Failed to start supervisor: {:?}", err);
            NodeError::StartFailed(err)
        })?;

        NodeActor {
            supervisor_child,
            supervisor_spec,
            restart_limiter: RestartLimiter::new(restart_intensity),
        }
        .run()
        .await
    }

    pub fn root_supervisor(&self) -> &ChildSpec<SupervisorBlueprint> {
        &self.supervisor_spec
    }
}

impl NodeActor {
    async fn run(mut self) -> Result<(), NodeError> {
        loop {
            let supervisor_exit = tokio::select! {
                res = &mut self.supervisor_child => res,
                _ = wait_for_shutdown_signal() => {
                    tracing::info!("Received Ctrl+C signal. Shutting down node.");
                    return self.exit_gracefully().await;
                }
            };

            match supervisor_exit {
                Ok(()) => {
                    tracing::info!("Root-Supervisor exited gracefully. Shutting down node.");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    return Ok(());
                }
                Err(err) => {
                    if !self.restart_limiter.acquire_permit() {
                        tracing::error!("Root-Supervisor exited with error: {:?}", err);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        return Err(NodeError::SupervisorExited(err));
                    } else {
                        tracing::warn!(
                            "Root-Supervisor exited with error: {:?}. Restarting...",
                            err
                        );
                        let new_supervisor_child = self
                            .supervisor_spec
                            .start()
                            .await
                            .map_err(NodeError::StartFailed)?;
                        self.supervisor_child = new_supervisor_child;
                    }
                }
            }
        }
    }

    async fn exit_gracefully(self) -> Result<(), NodeError> {
        let timeout = self.supervisor_spec.cfg().abort_timeout;

        tokio::select! {
            exit = self.supervisor_child.shutdown_abort(timeout) => {
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
