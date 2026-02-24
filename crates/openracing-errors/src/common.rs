//! Common error types and utilities used across all OpenRacing crates.
//!
//! This module provides the top-level error enum that can wrap all sub-errors,
//! along with error classification, severity levels, and utility traits.

use core::fmt;

use crate::{DeviceError, ProfileError, RTError, ValidationError};

/// Top-level error type that can wrap all OpenRacing sub-errors.
///
/// This enum provides a unified error type for the entire OpenRacing project,
/// allowing easy error propagation and classification.
#[derive(Debug, thiserror::Error)]
pub enum OpenRacingError {
    /// Real-time operation errors
    #[error("RT error: {0}")]
    RT(#[from] RTError),

    /// Device and hardware errors
    #[error("Device error: {0}")]
    Device(#[from] DeviceError),

    /// Profile and configuration errors
    #[error("Profile error: {0}")]
    Profile(#[from] ProfileError),

    /// Validation errors
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    /// I/O errors
    #[error("I/O error: {0}")]
    Io(#[source] std::io::Error),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Generic error with context
    #[error("{0}")]
    Other(String),
}

impl OpenRacingError {
    /// Get the error category for classification.
    pub fn category(&self) -> ErrorCategory {
        match self {
            OpenRacingError::RT(_) => ErrorCategory::RT,
            OpenRacingError::Device(_) => ErrorCategory::Device,
            OpenRacingError::Profile(_) => ErrorCategory::Profile,
            OpenRacingError::Validation(_) => ErrorCategory::Validation,
            OpenRacingError::Io(_) => ErrorCategory::IO,
            OpenRacingError::Config(_) => ErrorCategory::Config,
            OpenRacingError::Other(_) => ErrorCategory::Other,
        }
    }

    /// Get the error severity level.
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            OpenRacingError::RT(e) => e.severity(),
            OpenRacingError::Device(e) => e.severity(),
            OpenRacingError::Profile(e) => e.severity(),
            OpenRacingError::Validation(e) => e.severity(),
            OpenRacingError::Io(_) => ErrorSeverity::Error,
            OpenRacingError::Config(_) => ErrorSeverity::Error,
            OpenRacingError::Other(_) => ErrorSeverity::Error,
        }
    }

    /// Check if this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        self.severity() < ErrorSeverity::Critical
    }

    /// Create a configuration error with a message.
    pub fn config(msg: impl Into<String>) -> Self {
        OpenRacingError::Config(msg.into())
    }

    /// Create a generic error with a message.
    pub fn other(msg: impl Into<String>) -> Self {
        OpenRacingError::Other(msg.into())
    }
}

impl From<std::io::Error> for OpenRacingError {
    fn from(e: std::io::Error) -> Self {
        OpenRacingError::Io(e)
    }
}

/// Error category for classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ErrorCategory {
    /// Real-time operation errors
    RT = 0,
    /// Device and hardware errors
    Device = 1,
    /// Profile and configuration errors
    Profile = 2,
    /// Configuration errors
    Config = 3,
    /// I/O errors
    IO = 4,
    /// Validation errors
    Validation = 5,
    /// Plugin errors
    Plugin = 6,
    /// Telemetry errors
    Telemetry = 7,
    /// Other errors
    Other = 255,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCategory::RT => write!(f, "RT"),
            ErrorCategory::Device => write!(f, "Device"),
            ErrorCategory::Profile => write!(f, "Profile"),
            ErrorCategory::Config => write!(f, "Config"),
            ErrorCategory::IO => write!(f, "IO"),
            ErrorCategory::Validation => write!(f, "Validation"),
            ErrorCategory::Plugin => write!(f, "Plugin"),
            ErrorCategory::Telemetry => write!(f, "Telemetry"),
            ErrorCategory::Other => write!(f, "Other"),
        }
    }
}

/// Error severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ErrorSeverity {
    /// Informational, no action required
    Info = 0,
    /// Warning, may require attention
    Warning = 1,
    /// Error, operation failed
    Error = 2,
    /// Critical, system may be in unstable state
    Critical = 3,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorSeverity::Info => write!(f, "INFO"),
            ErrorSeverity::Warning => write!(f, "WARN"),
            ErrorSeverity::Error => write!(f, "ERROR"),
            ErrorSeverity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Context information for errors.
///
/// Provides additional context for error messages, useful for debugging
/// and error reporting.
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// The operation that was being performed
    pub operation: String,
    /// Additional context key-value pairs
    pub context: Vec<(String, String)>,
    /// Source location (file:line)
    pub location: Option<String>,
}

impl ErrorContext {
    /// Create a new error context for an operation.
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            context: Vec::new(),
            location: None,
        }
    }

    /// Add a context key-value pair.
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.push((key.into(), value.into()));
        self
    }

    /// Set the source location.
    pub fn at(mut self, file: impl Into<String>, line: u32) -> Self {
        self.location = Some(format!("{}:{}", file.into(), line));
        self
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "operation: {}", self.operation)?;
        for (key, value) in &self.context {
            write!(f, ", {}: {}", key, value)?;
        }
        if let Some(ref loc) = self.location {
            write!(f, " at {}", loc)?;
        }
        Ok(())
    }
}

/// Extension trait for adding context to errors.
pub trait ResultExt<T> {
    /// Add context to an error.
    fn context(self, ctx: ErrorContext) -> Result<T, OpenRacingError>;

    /// Add context with an operation name.
    fn with_context(self, operation: impl Into<String>) -> Result<T, OpenRacingError>;
}

impl<T, E: Into<OpenRacingError>> ResultExt<T> for std::result::Result<T, E> {
    fn context(self, ctx: ErrorContext) -> Result<T, OpenRacingError> {
        self.map_err(|e| {
            let err: OpenRacingError = e.into();
            OpenRacingError::Other(format!("{}: {}", ctx, err))
        })
    }

    fn with_context(self, operation: impl Into<String>) -> Result<T, OpenRacingError> {
        self.context(ErrorContext::new(operation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_category_display() {
        assert_eq!(ErrorCategory::RT.to_string(), "RT");
        assert_eq!(ErrorCategory::Device.to_string(), "Device");
        assert_eq!(ErrorCategory::Profile.to_string(), "Profile");
    }

    #[test]
    fn test_error_severity_ordering() {
        assert!(ErrorSeverity::Critical > ErrorSeverity::Error);
        assert!(ErrorSeverity::Error > ErrorSeverity::Warning);
        assert!(ErrorSeverity::Warning > ErrorSeverity::Info);
    }

    #[test]
    fn test_error_context() {
        let ctx = ErrorContext::new("load_profile")
            .with("profile_id", "test-123")
            .with("device", "moza-r9");
        assert!(ctx.to_string().contains("load_profile"));
        assert!(ctx.to_string().contains("profile_id"));
    }

    #[test]
    fn test_openracing_error_category() {
        let err: OpenRacingError = RTError::DeviceDisconnected.into();
        assert_eq!(err.category(), ErrorCategory::RT);

        let err = OpenRacingError::config("test");
        assert_eq!(err.category(), ErrorCategory::Config);
    }

    #[test]
    fn test_openracing_error_is_std_error() {
        let err: OpenRacingError = RTError::TimingViolation.into();
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_result_ext() {
        let result: std::result::Result<(), RTError> = Err(RTError::DeviceDisconnected);
        let with_ctx = result.with_context("test_operation");
        assert!(with_ctx.is_err());
        let err = with_ctx.unwrap_err();
        assert!(err.to_string().contains("test_operation"));
    }
}
