//! Unified error type for the service layer.

/// Unified error for the service layer.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// No document is currently open.
    #[error("no document is open")]
    NoDocument,
    /// A node id does not exist in the scene.
    #[error("node not found: {0}")]
    NodeNotFound(u64),
    /// A resource id is not registered.
    #[error("resource not found: {0}")]
    ResourceNotFound(u64),
    /// A task id is unknown.
    #[error("task not found: {0}")]
    TaskNotFound(u64),
    /// Underlying I/O failure (open / save).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON (de)serialization failure (`.kld` documents).
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// Error from the core image data model.
    #[error("image error: {0}")]
    Image(#[from] kaleido_core::ImageError),
    /// Error from the `.kld` document format.
    #[error("kld error: {0}")]
    Kld(#[from] kaleido_core::KldError),
    /// A Cordis service interaction failed.
    #[error("cordis error: {0}")]
    Cordis(String),
    /// A caller-supplied argument is invalid.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    /// Any other operation failure.
    #[error("operation failed: {0}")]
    Other(String),
}

/// Convenience result alias for service operations.
pub type ServiceResult<T> = Result<T, ServiceError>;
