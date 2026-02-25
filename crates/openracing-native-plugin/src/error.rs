//! Error types for native plugin loading.

use std::path::PathBuf;
use thiserror::Error;

/// Error type for native plugin operations.
#[derive(Error, Debug)]
pub enum NativePluginError {
    /// Plugin loading failed.
    #[error("Plugin loading failed: {0}")]
    LoadingFailed(String),

    /// ABI version mismatch.
    #[error("ABI version mismatch: expected {expected}, got {actual}")]
    AbiMismatch {
        /// Expected ABI version.
        expected: u32,
        /// Actual ABI version from plugin.
        actual: u32,
    },

    /// Plugin signature verification failed.
    #[error("Signature verification failed: {0}")]
    SignatureVerificationFailed(String),

    /// Plugin is unsigned and unsigned plugins are not allowed.
    #[error("Plugin is unsigned: {path}")]
    UnsignedPlugin {
        /// Path to the plugin.
        path: PathBuf,
    },

    /// Plugin signer is not trusted.
    #[error("Untrusted signer: {fingerprint}")]
    UntrustedSigner {
        /// Fingerprint of the signing key.
        fingerprint: String,
    },

    /// Plugin signer is distrusted.
    #[error("Distrusted signer: {fingerprint}")]
    DistrustedSigner {
        /// Fingerprint of the signing key.
        fingerprint: String,
    },

    /// Library loading error.
    #[error("Library loading error: {0}")]
    LibraryError(#[from] libloading::Error),

    /// Shared memory error.
    #[error("Shared memory error: {0}")]
    SharedMemoryError(String),

    /// IPC error.
    #[error("IPC error: {0}")]
    IpcError(String),

    /// Plugin initialization failed.
    #[error("Plugin initialization failed: {0}")]
    InitializationFailed(String),

    /// Plugin execution timeout.
    #[error("Plugin execution timeout after {duration_us}μs")]
    ExecutionTimeout {
        /// Duration in microseconds.
        duration_us: u64,
    },

    /// Budget violation.
    #[error("Budget violation: used {used_us}μs, budget {budget_us}μs")]
    BudgetViolation {
        /// Actual time used in microseconds.
        used_us: u32,
        /// Budget in microseconds.
        budget_us: u32,
    },

    /// Plugin crashed.
    #[error("Plugin crashed: {reason}")]
    Crashed {
        /// Reason for the crash.
        reason: String,
    },

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error.
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Error type for plugin loading operations.
#[derive(Error, Debug, Clone)]
pub enum NativePluginLoadError {
    /// ABI version mismatch between plugin and loader.
    #[error("ABI version mismatch: expected {expected}, got {actual}")]
    AbiMismatch {
        /// Expected ABI version.
        expected: u32,
        /// Actual ABI version from plugin.
        actual: u32,
    },

    /// Plugin signature is invalid.
    #[error("Invalid signature: {reason}")]
    InvalidSignature {
        /// Reason for the failure.
        reason: String,
    },

    /// Plugin is unsigned but unsigned plugins are not allowed.
    #[error("Plugin is unsigned: {path}")]
    UnsignedPlugin {
        /// Path to the plugin.
        path: String,
    },

    /// Plugin signer is not trusted.
    #[error("Untrusted signer: {fingerprint}")]
    UntrustedSigner {
        /// Fingerprint of the signing key.
        fingerprint: String,
    },

    /// Library loading failed.
    #[error("Library loading failed: {reason}")]
    LibraryLoadFailed {
        /// Reason for the failure.
        reason: String,
    },

    /// Plugin initialization failed.
    #[error("Initialization failed: {reason}")]
    InitializationFailed {
        /// Reason for the failure.
        reason: String,
    },
}

impl From<NativePluginLoadError> for NativePluginError {
    fn from(err: NativePluginLoadError) -> Self {
        match err {
            NativePluginLoadError::AbiMismatch { expected, actual } => {
                NativePluginError::AbiMismatch { expected, actual }
            }
            NativePluginLoadError::InvalidSignature { reason } => {
                NativePluginError::SignatureVerificationFailed(reason)
            }
            NativePluginLoadError::UnsignedPlugin { path } => NativePluginError::UnsignedPlugin {
                path: PathBuf::from(path),
            },
            NativePluginLoadError::UntrustedSigner { fingerprint } => {
                NativePluginError::UntrustedSigner { fingerprint }
            }
            NativePluginLoadError::LibraryLoadFailed { reason } => {
                NativePluginError::LoadingFailed(reason)
            }
            NativePluginLoadError::InitializationFailed { reason } => {
                NativePluginError::InitializationFailed(reason)
            }
        }
    }
}
