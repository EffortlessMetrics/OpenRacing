//! Prelude for openracing-crypto
//!
//! This module re-exports the most commonly used types and traits for convenience.
//!
//! # Example
//!
//! ```
//! use openracing_crypto::prelude::*;
//!
//! // Now you have access to all the main types
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let keypair = KeyPair::generate()?;
//! # Ok(())
//! # }
//! ```

pub use crate::ed25519::{Ed25519Signer, Ed25519Verifier, KeyPair, PublicKey, Signature};
pub use crate::error::{CryptoError, CryptoResult};
pub use crate::trust_store::{ImportResult, TrustEntry, TrustStore, TrustStoreStats};
pub use crate::verification::{
    ContentType, VerificationConfig, VerificationReport, VerificationResult, VerificationService,
};
pub use crate::{SignatureMetadata, SignatureVerifier, TrustLevel};
