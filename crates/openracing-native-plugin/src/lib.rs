//! Native plugin loading with signature verification and RT communication.
//!
//! This crate provides secure native plugin loading for OpenRacing with:
//! - Ed25519 signature verification against a trust store
//! - ABI version compatibility checking
//! - Support for both signed and unsigned plugins (configurable)
//! - SPSC shared memory for RT communication
//! - Cross-platform support (Windows, Linux, macOS)
//!
//! # Security Considerations
//!
//! Native plugins run with full process privileges and are NOT sandboxed.
//! Only load plugins from trusted sources with verified signatures.
//!
//! ## Secure-by-Default Configuration
//!
//! The default configuration requires:
//! - Valid Ed25519 signature
//! - Trusted signer in the trust store
//! - Matching ABI version
//!
//! ## Configuration Modes
//!
//! | Mode | `require_signatures` | `allow_unsigned` | Use Case |
//! |------|---------------------|------------------|----------|
//! | Strict | `true` | `false` | Production (default) |
//! | Permissive | `true` | `true` | Development with mixed plugins |
//! | Development | `false` | `true` | Testing only |
//!
//! # Example
//!
//! ```rust,no_run
//! use openracing_native_plugin::{
//!     NativePluginHost, NativePluginConfig, NativePluginLoader,
//! };
//! use openracing_crypto::trust_store::TrustStore;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a trust store with trusted keys
//!     let trust_store = TrustStore::new_in_memory();
//!     
//!     // Use secure defaults (require signatures, no unsigned plugins)
//!     let host = NativePluginHost::new(trust_store, NativePluginConfig::default());
//!     
//!     // Or create a permissive development configuration
//!     let dev_host = NativePluginHost::new_permissive_for_development();
//!     
//!     Ok(())
//! }
//! ```

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![warn(missing_docs, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod abi_check;
pub mod error;
pub mod loader;
pub mod plugin;
pub mod prelude;
pub mod signature;
pub mod spsc;

#[cfg(windows)]
#[cfg_attr(docsrs, doc(cfg(windows)))]
pub mod windows;

#[cfg(target_os = "linux")]
#[cfg_attr(docsrs, doc(cfg(target_os = "linux")))]
pub mod linux;

#[cfg(target_os = "macos")]
#[cfg_attr(docsrs, doc(cfg(target_os = "macos")))]
pub mod macos;

pub use abi_check::{AbiCheckResult, CURRENT_ABI_VERSION, check_abi_compatibility};
pub use error::{NativePluginError, NativePluginLoadError};
pub use loader::{NativePluginConfig, NativePluginHost, NativePluginLoader};
pub use plugin::{NativePlugin, PluginFrame, PluginVTable, SharedMemoryHeader};
pub use signature::{SignatureVerificationConfig, SignatureVerificationResult, SignatureVerifier};
pub use spsc::{SpscChannel, SpscReader, SpscWriter};
