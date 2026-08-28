use crate::image::PixelFormat;
use thiserror::Error;

/// Core error type for all kaleido operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ImageError {
    #[error("Invalid image dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    #[error("Image data length mismatch: expected at least {expected}, got {actual}")]
    DataLengthMismatch { expected: usize, actual: usize },

    #[error("Pixel out of bounds: ({x}, {y}) for image {width}x{height}")]
    OutOfBounds {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },

    #[error("Invalid row stride: {stride} (minimum required: {min_required})")]
    InvalidRowStride { stride: u32, min_required: u32 },

    #[error("Unsupported pixel format: {format:?}")]
    UnsupportedFormat { format: PixelFormat },

    #[error("Offset {offset} out of bounds for data buffer of length {len}")]
    InvalidOffset { offset: usize, len: usize },

    #[error("Operation not supported for format {format:?}: {reason}")]
    UnsupportedOperation { format: PixelFormat, reason: String },

    #[error("Layer not found: {id}")]
    LayerNotFound { id: String },

    #[error("Layer already exists: {id}")]
    LayerAlreadyExists { id: String },

    #[error("Invalid layer index: {index}")]
    InvalidLayerIndex { index: usize },

    #[error("Image is empty (no data)")]
    EmptyImage,

    #[error("Operation failed: {reason}")]
    OperationFailed { reason: String },
}

/// Convenience result type for image operations.
pub type ImageResult<T> = Result<T, ImageError>;
