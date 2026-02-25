//! IPC-specific error types

use std::io;
use thiserror::Error;

/// IPC error type
#[derive(Debug, Error)]
pub enum IpcError {
    /// Transport initialization failed
    #[error("Transport initialization failed: {0}")]
    TransportInit(String),

    /// Connection failed
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Message encoding failed
    #[error("Message encoding failed: {0}")]
    EncodingFailed(String),

    /// Message decoding failed
    #[error("Message decoding failed: {0}")]
    DecodingFailed(String),

    /// Version incompatibility
    #[error("Version incompatibility: client {client} is not compatible with server {server}")]
    VersionIncompatibility {
        /// Client version
        client: String,
        /// Server version
        server: String,
    },

    /// Feature negotiation failed
    #[error("Feature negotiation failed: {0}")]
    FeatureNegotiation(String),

    /// Server not running
    #[error("Server is not running")]
    ServerNotRunning,

    /// Connection limit exceeded
    #[error("Connection limit exceeded: max {max} connections")]
    ConnectionLimitExceeded {
        /// Maximum connections allowed
        max: usize,
    },

    /// Timeout exceeded
    #[error("Operation timed out after {timeout_ms}ms")]
    Timeout {
        /// Timeout in milliseconds
        timeout_ms: u64,
    },

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// gRPC error
    #[error("gRPC error: {0}")]
    Grpc(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Platform not supported
    #[error("Platform not supported for transport: {0}")]
    PlatformNotSupported(String),

    /// Shutdown requested
    #[error("Server shutdown requested")]
    ShutdownRequested,
}

impl IpcError {
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            IpcError::ConnectionFailed(_)
                | IpcError::Timeout { .. }
                | IpcError::VersionIncompatibility { .. }
                | IpcError::FeatureNegotiation(_)
        )
    }

    /// Check if this error indicates the server should stop
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            IpcError::TransportInit(_) | IpcError::ServerNotRunning | IpcError::ShutdownRequested
        )
    }

    /// Create a timeout error
    pub fn timeout(timeout_ms: u64) -> Self {
        IpcError::Timeout { timeout_ms }
    }

    /// Create a connection limit error
    pub fn connection_limit(max: usize) -> Self {
        IpcError::ConnectionLimitExceeded { max }
    }
}

/// Specialized Result type for IPC operations
pub type IpcResult<T> = std::result::Result<T, IpcError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_is_recoverable() {
        let err = IpcError::ConnectionFailed("test".to_string());
        assert!(err.is_recoverable());

        let err = IpcError::TransportInit("test".to_string());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_error_is_fatal() {
        let err = IpcError::TransportInit("test".to_string());
        assert!(err.is_fatal());

        let err = IpcError::ConnectionFailed("test".to_string());
        assert!(!err.is_fatal());
    }

    #[test]
    fn test_error_helpers() {
        let err = IpcError::timeout(1000);
        assert!(matches!(err, IpcError::Timeout { timeout_ms: 1000 }));

        let err = IpcError::connection_limit(100);
        assert!(matches!(
            err,
            IpcError::ConnectionLimitExceeded { max: 100 }
        ));
    }
}
