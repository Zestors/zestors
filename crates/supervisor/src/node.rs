use crate::_prelude::*;
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;
use zestors_runtime::errors::{JoinError, ShutdownAbortError};
use zestors_supervision::{RestartIntensity, StartOnError};

/// The reason a [`Node`] stopped running.
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    /// The root supervisor failed to start in the first place.
    #[error("Failed to start root supervisor: {0}")]
    StartFailed(#[source] StartOnError),

    /// The root supervisor exited with an error, and its restart budget
    /// (see [`Node::with_restart_intensity`]) was exhausted.
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

/// Runs a single root [`Supervisor`] as an entire program: starts it,
/// restarts it (up to a configurable limit) if it exits with an error, and
/// shuts it down gracefully on a Ctrl+C/SIGTERM — forcing immediate
/// termination if a second signal arrives before it finishes.
pub struct Node {
    restart_intensity: RestartIntensity,
    supervisor_spec: ChildSpec<SupervisorBlueprint>,
    exit_delay: Duration,
    shutdown: Arc<watch::Sender<bool>>,
}

/// Asks a [`Node`] to shut down gracefully, exactly as if it had received
/// Ctrl+C. Obtained from [`Node::shutdown_handle`].
///
/// Unlike signalling the root supervisor's address, this works at any point,
/// also before [`Node::run`] has started the supervisor: the request is
/// remembered, and the node shuts down as soon as it is running. (An actor that
/// hasn't been started yet rejects signals, so a signal sent to the root
/// supervisor too early is dropped.)
#[derive(Clone, Debug)]
pub struct NodeShutdown {
    shutdown: Arc<watch::Sender<bool>>,
}

impl NodeShutdown {
    /// Requests a graceful shutdown. Does nothing if one was already requested.
    pub fn shutdown(&self) {
        self.shutdown.send_replace(true);
    }
}

struct NodeActor {
    shutdown: watch::Receiver<bool>,
    supervisor_child: Child<(), SupervisorInterface>,
    restart_limiter: RestartLimiter,
    supervisor_spec: ChildSpec<SupervisorBlueprint>,
    exit_delay: Duration,
}

impl Node {
    /// Creates a [`Node`] for the root supervisor described by `spec`. By
    /// default the root supervisor is never restarted; see
    /// [`Node::with_restart_intensity`].
    pub fn new(spec: ChildSpec<SupervisorBlueprint>) -> Self {
        Self {
            restart_intensity: RestartIntensity::restarts(0),
            supervisor_spec: spec,
            exit_delay: Duration::from_secs(1),
            shutdown: Arc::new(watch::channel(false).0),
        }
    }

    /// Returns a handle that shuts this node down gracefully, usable before
    /// and after [`Node::run`] (which consumes the node, so take it first).
    pub fn shutdown_handle(&self) -> NodeShutdown {
        NodeShutdown {
            shutdown: self.shutdown.clone(),
        }
    }

    /// Sets how many times (and how often) the root supervisor may be
    /// restarted if it exits with an error, before [`Node::run`] gives up
    /// and returns [`NodeError::SupervisorExited`].
    pub fn with_restart_intensity(mut self, intensity: RestartIntensity) -> Self {
        self.restart_intensity = intensity;
        self
    }

    /// Sets how long [`Node::run`] lingers after the root supervisor has
    /// exited, giving log output a chance to be flushed before the process ends.
    /// Defaults to one second.
    pub fn with_exit_delay(mut self, delay: Duration) -> Self {
        self.exit_delay = delay;
        self
    }

    /// Starts the root supervisor and runs until the node exits — either
    /// because the supervisor exited normally, a shutdown signal was
    /// handled, or an unrecoverable error occurred (see [`NodeError`]).
    pub async fn run(self) -> Result<(), NodeError> {
        let Self {
            restart_intensity,
            supervisor_spec,
            exit_delay,
            shutdown,
        } = self;

        let supervisor_child = supervisor_spec.start().await.map_err(|err| {
            tracing::error!("Failed to start supervisor: {:?}", err);
            NodeError::StartFailed(err)
        })?;

        NodeActor {
            shutdown: shutdown.subscribe(),
            supervisor_child,
            supervisor_spec,
            exit_delay,
            restart_limiter: RestartLimiter::new(restart_intensity),
        }
        .run()
        .await
    }

    /// Returns the root supervisor's [`ChildSpec`].
    ///
    /// Signals sent to its address are only accepted once the supervisor is
    /// running (see [`Address::watch_running`](zestors_runtime::prelude::Address));
    /// use [`Node::shutdown_handle`] to request a shutdown that can't be missed.
    pub fn root_supervisor(&self) -> &ChildSpec<SupervisorBlueprint> {
        &self.supervisor_spec
    }
}

impl NodeActor {
    async fn run(mut self) -> Result<(), NodeError> {
        loop {
            let supervisor_exit = tokio::select! {
                res = &mut self.supervisor_child => Some(res),
                _ = wait_for_shutdown_signal() => {
                    tracing::info!("Received Ctrl+C signal. Shutting down node.");
                    None
                }
                Ok(_) = self.shutdown.wait_for(|requested| *requested) => {
                    tracing::info!("Shutdown requested. Shutting down node.");
                    None
                }
            };

            // Shutdown was requested: stop the supervisor gracefully.
            let Some(supervisor_exit) = supervisor_exit else {
                return self.exit_gracefully().await;
            };

            match supervisor_exit {
                Ok(()) => {
                    tracing::info!("Root-Supervisor exited gracefully. Shutting down node.");
                    tokio::time::sleep(self.exit_delay).await;
                    return Ok(());
                }
                Err(err) => {
                    if !self.restart_limiter.acquire_permit() {
                        tracing::error!("Root-Supervisor exited with error: {:?}", err);
                        tokio::time::sleep(self.exit_delay).await;
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
