use zestors_supervisor::NodeError;

/// Why a [`ClusterNode`](crate::ClusterNode) stopped running.
#[derive(Debug, thiserror::Error)]
pub enum ClusterNodeError {
    /// The network backend could not be started, for example because it
    /// could not listen on its address or rejected the node name.
    #[error("Failed to start the network backend: {0}")]
    Backend(#[source] std::io::Error),

    /// The generation could not be read from or recorded in the file given to
    /// [`ClusterConfig::generation_store`](crate::ClusterConfig::generation_store).
    #[error("Failed to use the generation store: {0}")]
    Generation(#[source] std::io::Error),

    /// The wrapped [`Node`](zestors_supervisor::Node) stopped with an error.
    #[error(transparent)]
    Node(#[from] NodeError),
}

impl ClusterNodeError {
    /// The process exit code this error should be reported with.
    pub fn exit_code(&self) -> i32 {
        match self {
            ClusterNodeError::Node(err) => err.exit_code(),
            ClusterNodeError::Backend(_) | ClusterNodeError::Generation(_) => 1,
        }
    }
}
