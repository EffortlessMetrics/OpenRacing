//! Tracing error types

use core::fmt;

/// Tracing errors
#[derive(Debug, thiserror::Error)]
pub enum TracingError {
    /// Platform not supported for native tracing
    #[error("Platform not supported for native tracing")]
    PlatformNotSupported,

    /// Provider initialization failed
    #[error("Tracing provider initialization failed: {0}")]
    InitializationFailed(String),

    /// Event emission failed
    #[error("Trace event emission failed: {0}")]
    EmissionFailed(String),

    /// Provider not initialized
    #[error("Tracing provider not initialized")]
    NotInitialized,

    /// Buffer overflow
    #[error("Trace buffer overflow: {0} events lost")]
    BufferOverflow(u64),

    /// Invalid configuration
    #[error("Invalid tracing configuration: {0}")]
    InvalidConfiguration(String),

    /// Platform-specific error
    #[error("Platform tracing error: {0}")]
    PlatformError(String),
}

impl TracingError {
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            TracingError::BufferOverflow(_) => true,
            TracingError::PlatformNotSupported => false,
            TracingError::InitializationFailed(_) => false,
            TracingError::EmissionFailed(_) => true,
            TracingError::NotInitialized => true,
            TracingError::InvalidConfiguration(_) => false,
            TracingError::PlatformError(_) => true,
        }
    }

    /// Check if this error indicates a missing platform feature
    pub fn is_platform_missing(&self) -> bool {
        matches!(self, TracingError::PlatformNotSupported)
    }

    /// Create an initialization error with context
    pub fn init_failed(context: impl fmt::Display) -> Self {
        TracingError::InitializationFailed(context.to_string())
    }

    /// Create an emission error with context
    pub fn emit_failed(context: impl fmt::Display) -> Self {
        TracingError::EmissionFailed(context.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_is_recoverable() {
        assert!(!TracingError::PlatformNotSupported.is_recoverable());
        assert!(TracingError::BufferOverflow(100).is_recoverable());
        assert!(TracingError::NotInitialized.is_recoverable());
    }

    #[test]
    fn test_error_platform_missing() {
        assert!(TracingError::PlatformNotSupported.is_platform_missing());
        assert!(!TracingError::InitializationFailed("test".into()).is_platform_missing());
    }

    #[test]
    fn test_error_constructors() {
        let e = TracingError::init_failed("test context");
        assert!(matches!(e, TracingError::InitializationFailed(_)));

        let e = TracingError::emit_failed("emit context");
        assert!(matches!(e, TracingError::EmissionFailed(_)));
    }

    #[test]
    fn test_error_display() {
        let e = TracingError::BufferOverflow(42);
        let s = e.to_string();
        assert!(s.contains("42"));
    }
}
