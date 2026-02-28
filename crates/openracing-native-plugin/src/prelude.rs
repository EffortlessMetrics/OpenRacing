//! Convenience re-exports for common types.

pub use crate::abi_check::{AbiCheckResult, CURRENT_ABI_VERSION, check_abi_compatibility};
pub use crate::error::{NativePluginError, NativePluginLoadError};
pub use crate::loader::{NativePluginConfig, NativePluginHost, NativePluginLoader};
pub use crate::plugin::{NativePlugin, PluginFrame, PluginVTable, SharedMemoryHeader};
pub use crate::signature::{
    SignatureVerificationConfig, SignatureVerificationResult, SignatureVerifier,
};
pub use crate::spsc::{SpscChannel, SpscReader, SpscWriter};
