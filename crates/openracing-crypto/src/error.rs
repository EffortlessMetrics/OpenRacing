//! Error types for cryptographic operations
//!
//! This module provides error types for all cryptographic operations in the crate.

#![deny(clippy::unwrap_used)]

use thiserror::Error;

/// Cryptographic operation errors
#[derive(Error, Debug)]
pub enum CryptoError {
    /// Invalid signature
    #[error("Invalid signature")]
    InvalidSignature,

    /// Untrusted signer
    #[error("Untrusted signer: {0}")]
    UntrustedSigner(String),

    /// Signature verification failed
    #[error("Signature verification failed: {0}")]
    VerificationFailed(String),

    /// Key format error
    #[error("Key format error: {0}")]
    KeyFormatError(String),

    /// I/O error
    #[error("File I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Trust store error
    #[error("Trust store error: {0}")]
    TrustStoreError(String),

    /// Constant-time comparison failed
    #[error("Constant-time comparison failed")]
    ConstantTimeError,

    /// Invalid key length
    #[error("Invalid key length: expected {expected} bytes, got {actual}")]
    InvalidKeyLength {
        /// Expected length
        expected: usize,
        /// Actual length
        actual: usize,
    },

    /// Invalid signature length
    #[error("Invalid signature length: expected {expected} bytes, got {actual}")]
    InvalidSignatureLength {
        /// Expected length
        expected: usize,
        /// Actual length
        actual: usize,
    },

    /// Key not found in trust store
    #[error("Key not found in trust store: {0}")]
    KeyNotFound(String),

    /// Cannot modify system key
    #[error("Cannot modify system key")]
    SystemKeyProtected,
}

impl From<serde_json::Error> for CryptoError {
    fn from(e: serde_json::Error) -> Self {
        CryptoError::SerializationError(e.to_string())
    }
}

impl From<base64::DecodeError> for CryptoError {
    fn from(e: base64::DecodeError) -> Self {
        CryptoError::KeyFormatError(format!("Base64 decode error: {}", e))
    }
}

/// Result type for cryptographic operations
pub type CryptoResult<T> = std::result::Result<T, CryptoError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = CryptoError::InvalidSignature;
        assert_eq!(err.to_string(), "Invalid signature");

        let err = CryptoError::UntrustedSigner("test-key".to_string());
        assert!(err.to_string().contains("test-key"));

        let err = CryptoError::InvalidKeyLength {
            expected: 32,
            actual: 16,
        };
        assert!(err.to_string().contains("32"));
        assert!(err.to_string().contains("16"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let crypto_err: CryptoError = io_err.into();
        assert!(matches!(crypto_err, CryptoError::IoError(_)));
    }

    #[test]
    fn test_error_from_json() {
        let json_result: Result<serde_json::Value, _> = serde_json::from_str("invalid json");
        match json_result {
            Err(json_err) => {
                let crypto_err: CryptoError = json_err.into();
                assert!(matches!(crypto_err, CryptoError::SerializationError(_)));
            }
            Ok(_) => panic!("Expected error"),
        }
    }
}
