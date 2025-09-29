//! Security module for signature verification and trust management
//! 
//! This module implements Ed25519 signature verification for:
//! - Application binaries and updates
//! - Firmware images
//! - Plugin packages
//! - Profile files (optional)

pub mod signature;
pub mod trust;
pub mod verification;

pub use signature::{Signature, SignatureError};
pub use trust::{TrustStore, TrustLevel};
pub use verification::{Verifier, VerificationResult};

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Security configuration for the racing wheel suite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Whether to require signatures for all components
    pub require_signatures: bool,
    /// Whether to allow unsigned plugins (development mode)
    pub allow_unsigned_plugins: bool,
    /// Whether to allow unsigned profiles
    pub allow_unsigned_profiles: bool,
    /// Path to the trust store
    pub trust_store_path: String,
    /// Minimum trust level required for automatic verification
    pub min_trust_level: TrustLevel,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            require_signatures: true,
            allow_unsigned_plugins: false,
            allow_unsigned_profiles: true, // Profiles are optional by default
            trust_store_path: "trust_store.json".to_string(),
            min_trust_level: TrustLevel::Trusted,
        }
    }
}

/// Initialize security subsystem
pub async fn init_security(config: &SecurityConfig) -> Result<Verifier, SecurityError> {
    let trust_store = TrustStore::load_or_create(&config.trust_store_path).await?;
    Ok(Verifier::new(trust_store, config.clone()))
}

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Signature verification failed: {0}")]
    VerificationFailed(String),
    #[error("Trust store error: {0}")]
    TrustStore(#[from] trust::TrustStoreError),
    #[error("Signature error: {0}")]
    Signature(#[from] SignatureError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}