//! Native plugin system with SPSC shared memory and RT watchdog
//!
//! This module re-exports types from the `openracing-native-plugin` crate
//! for backward compatibility.
//!
//! See the `openracing-native-plugin` crate for full documentation.

pub use openracing_native_plugin::{
    AbiCheckResult, CURRENT_ABI_VERSION, NativePlugin, NativePluginConfig, NativePluginError,
    NativePluginHost, NativePluginLoadError, NativePluginLoader, PluginFrame, PluginVTable,
    SharedMemoryHeader, SignatureVerificationConfig, SignatureVerificationResult,
    SignatureVerifier, SpscChannel, SpscReader, SpscWriter, check_abi_compatibility,
};
