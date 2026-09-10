use crate::_prelude::*;
use rootcause::report;
use std::{sync::OnceLock, time::Duration};

pub struct Node {
    restart_intensity: RestartIntensity,
    supervisor_spec: ChildSpec<SupervisorBlueprint>,
}

struct NodeActor {
    supervisor_child: Child<(), SupervisorInterface>,
    restart_limiter: RestartLimiter,
    supervisor_spec: ChildSpec<SupervisorBlueprint>,
}

static ROOT_SUPERVISOR_PID: OnceLock<ChildDescription> = OnceLock::new();

impl Node {
    pub fn root_supervisor() -> Option<&'static ChildDescription> {
        ROOT_SUPERVISOR_PID.get()
    }

    pub fn new(supervisor_spec: ChildSpec<SupervisorBlueprint>) -> Self {
        Self {
            restart_intensity: RestartIntensity::new(3, Duration::from_secs(120)),
            supervisor_spec,
        }
    }

    pub fn with_restart_intensity(mut self, intensity: RestartIntensity) -> Self {
        self.restart_intensity = intensity;
        self
    }

    pub async fn start(self) -> Result<Address<SupervisorInterface>, Report> {
        let Self {
            restart_intensity,
            supervisor_spec,
        } = self;

        ROOT_SUPERVISOR_PID
            .set(ChildDescription {
                pid: supervisor_spec.pid().clone(),
                cfg: supervisor_spec.cfg().clone(),
            })
            .map_err(|_| report!("Root Node can only be started once."))?;

        let supervisor_child = supervisor_spec.start().await?;
        let supervisor_address = supervisor_child.address().clone();

        tokio::spawn(
            NodeActor {
                supervisor_child,
                supervisor_spec,
                restart_limiter: RestartLimiter::new(restart_intensity),
            }
            .run(),
        );

        Ok(supervisor_address)
    }
}

impl NodeActor {
    pub async fn run(mut self) {
        loop {
            let supervisor_exit = tokio::select! {
                res = &mut self.supervisor_child => res,
                _ = wait_for_shutdown_signal() => {
                    tracing::info!("Received Ctrl+C signal. Shutting down node.");
                    self.exit_gracefully().await;
                }
            };

            match supervisor_exit {
                Ok(()) => {
                    tracing::info!("Root-Supervisor exited gracefully. Shutting down node.");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    std::process::exit(0);
                }
                Err(err) => {
                    if !self.restart_limiter.allow_restart() {
                        tracing::error!("Root-Supervisor exited with error: {:?}", err);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        std::process::exit(1);
                    } else {
                        tracing::warn!(
                            "Root-Supervisor exited with error: {:?}. Restarting...",
                            err
                        );
                        let new_supervisor_child = self.supervisor_spec.start().await.unwrap();
                        self.supervisor_child = new_supervisor_child;
                    }
                }
            }
        }
    }

    async fn exit_gracefully(self) -> ! {
        let timeout = self.supervisor_spec.cfg().abort_timeout;

        tokio::select! {
            exit = self.supervisor_child.shutdown_abort(timeout) => {
                match exit {
                    Ok(()) => {
                        tracing::info!("Root-Supervisor exited gracefully. Shutting down node.");
                        std::process::exit(0)
                    }
                    Err(err) => {
                        tracing::error!("Root-Supervisor failed to exit gracefully within timeout: {:?}", err);
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        std::process::exit(1)
                    }
                }
            }
            _ = wait_for_shutdown_signal() => {
                tracing::warn!("Received second shutdown signal. Forcing immediate termination.");
                std::process::exit(130)
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
