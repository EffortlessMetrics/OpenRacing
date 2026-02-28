//! Profile storage and management for OpenRacing
//!
//! This crate provides a complete profile persistence system with:
//! - JSON Schema validation with line/column error reporting
//! - Profile migration system for schema version upgrades
//! - Ed25519 signature verification for profile authenticity
//! - Deterministic profile merge with Global→Game→Car→Session hierarchy
//! - File-based storage with atomic operations
//!
//! # Architecture
//!
//! The crate is organized into several modules:
//!
//! - [`repository`]: Core `ProfileRepository` struct and operations
//! - [`storage`]: File-based storage operations with atomic writes
//! - [`validation`]: Profile validation logic and schema checking
//! - [`migration`]: Profile migration system integration
//! - [`signature`]: Ed25519 profile signing and verification
//! - [`error`]: Error types for repository operations
//!
//! # Error Recovery
//!
//! All operations follow a consistent error recovery pattern:
//! - File operations use atomic writes (write to temp, then rename)
//! - Failed migrations preserve original files via backups
//! - Signature verification failures return appropriate trust states
//!
//! # Example
//!
//! ```ignore
//! use openracing_profile_repository::prelude::*;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create a repository
//! let config = ProfileRepositoryConfig {
//!     profiles_dir: "profiles".into(),
//!     trusted_keys: vec![],
//!     auto_migrate: true,
//!     backup_on_migrate: true,
//! };
//! let repo = ProfileRepository::new(config).await?;
//!
//! // Save a profile
//! repo.save_profile(&profile, None).await?;
//!
//! // Load and resolve profiles
//! let resolved = repo.resolve_profile_hierarchy(
//!     Some("iracing"),
//!     Some("gt3"),
//!     None,
//!     None
//! ).await?;
//! # Ok(())
//! # }
//! ```

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![warn(missing_docs, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod error;
pub mod migration;
pub mod prelude;
pub mod repository;
pub mod signature;
pub mod storage;
pub mod validation;

pub use error::{ProfileRepositoryError, StorageError, ValidationError};
pub use repository::{ProfileRepository, ProfileRepositoryConfig};
pub use signature::{ProfileSignature, ProfileSigner, TrustState};
pub use storage::FileStorage;
pub use validation::ProfileValidationContext;

/// Result type for repository operations
pub type Result<T> = std::result::Result<T, ProfileRepositoryError>;
