//! Diagnostic error types
//!
//! Provides error handling for recording, replay, and bundle operations.

use thiserror::Error;

/// Diagnostic operation error
#[derive(Debug, Clone, Error)]
pub enum DiagnosticError {
    /// Recording error
    #[error("Recording error: {0}")]
    Recording(String),

    /// Replay error
    #[error("Replay error: {0}")]
    Replay(String),

    /// File format error
    #[error("File format error: {0}")]
    Format(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Compression error
    #[error("Compression error: {0}")]
    Compression(String),

    /// Size limit exceeded
    #[error("Size limit exceeded: {0}")]
    SizeLimit(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    Configuration(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// CRC mismatch
    #[error("CRC mismatch: expected {expected}, got {actual}")]
    CrcMismatch {
        /// Expected CRC value
        expected: u32,
        /// Actual CRC value
        actual: u32,
    },

    /// Invalid magic number
    #[error("Invalid magic number: expected {expected:?}, got {actual:?}")]
    InvalidMagic {
        /// Expected magic bytes
        expected: [u8; 4],
        /// Actual magic bytes
        actual: [u8; 4],
    },

    /// Unsupported version
    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u32),
}

impl From<std::io::Error> for DiagnosticError {
    fn from(err: std::io::Error) -> Self {
        DiagnosticError::Io(err.to_string())
    }
}

impl From<bincode::error::EncodeError> for DiagnosticError {
    fn from(err: bincode::error::EncodeError) -> Self {
        DiagnosticError::Serialization(err.to_string())
    }
}

impl From<bincode::error::DecodeError> for DiagnosticError {
    fn from(err: bincode::error::DecodeError) -> Self {
        DiagnosticError::Deserialization(err.to_string())
    }
}

impl From<serde_json::Error> for DiagnosticError {
    fn from(err: serde_json::Error) -> Self {
        DiagnosticError::Serialization(err.to_string())
    }
}

/// Result type for diagnostic operations
pub type DiagnosticResult<T> = Result<T, DiagnosticError>;
