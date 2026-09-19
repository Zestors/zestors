use zestors_supervisor::NodeError;

/// Why a [`ClusterNode`](crate::ClusterNode) stopped running.
#[derive(Debug, thiserror::Error)]
pub enum ClusterNodeError {
    /// The node name is not a valid DNS name, so it can't be used with TLS.
    #[error("Invalid node name {0:?}: must be a valid DNS name")]
    InvalidNodeName(String),

    /// The QUIC endpoint could not be started.
    #[error("Failed to bind cluster endpoint: {0}")]
    Bind(#[source] std::io::Error),

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
            ClusterNodeError::InvalidNodeName(_)
            | ClusterNodeError::Bind(_)
            | ClusterNodeError::Generation(_) => 1,
        }
    }
}
