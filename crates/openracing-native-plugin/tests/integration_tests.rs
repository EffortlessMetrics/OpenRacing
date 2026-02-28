//! Integration tests for native plugin loading.

use openracing_crypto::trust_store::TrustStore;
use openracing_native_plugin::{
    AbiCheckResult, CURRENT_ABI_VERSION, NativePluginConfig, NativePluginHost, NativePluginLoader,
    check_abi_compatibility,
};

#[tokio::test]
async fn test_host_creation() {
    let host = NativePluginHost::new_with_defaults();
    assert_eq!(host.plugin_count().await, 0);
}

#[tokio::test]
async fn test_host_development_mode() {
    let host = NativePluginHost::new_permissive_for_development();
    assert!(host.config().allow_unsigned);
    assert!(!host.config().require_signatures);
}

#[tokio::test]
async fn test_host_config_update() {
    let trust_store = TrustStore::new_in_memory();
    let mut host = NativePluginHost::new(trust_store, NativePluginConfig::development());

    assert!(host.config().allow_unsigned);

    host.set_config(NativePluginConfig::strict());
    assert!(!host.config().allow_unsigned);
}

#[test]
fn test_loader_creation() {
    let trust_store = TrustStore::new_in_memory();
    let loader = NativePluginLoader::with_defaults(&trust_store);
    assert!(!loader.config().allow_unsigned);
}

#[test]
fn test_loader_with_config() {
    let trust_store = TrustStore::new_in_memory();
    let loader = NativePluginLoader::new(&trust_store, NativePluginConfig::development());
    assert!(loader.config().allow_unsigned);
    assert!(!loader.config().require_signatures);
}

#[test]
fn test_abi_check_compatible() {
    let result = check_abi_compatibility(CURRENT_ABI_VERSION);
    assert_eq!(result, AbiCheckResult::Compatible);
}

#[test]
fn test_abi_check_incompatible() {
    let result = check_abi_compatibility(999);
    assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
}

#[test]
fn test_config_default_is_strict() {
    let config = NativePluginConfig::default();
    assert!(!config.allow_unsigned);
    assert!(config.require_signatures);
}

#[test]
fn test_config_presets() {
    let strict = NativePluginConfig::strict();
    assert!(!strict.allow_unsigned);
    assert!(strict.require_signatures);

    let permissive = NativePluginConfig::permissive();
    assert!(permissive.allow_unsigned);
    assert!(permissive.require_signatures);

    let dev = NativePluginConfig::development();
    assert!(dev.allow_unsigned);
    assert!(!dev.require_signatures);
}

#[tokio::test]
async fn test_unload_nonexistent_plugin() {
    let host = NativePluginHost::new_with_defaults();
    let result = host.unload_plugin(uuid::Uuid::new_v4()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_is_loaded() {
    let host = NativePluginHost::new_with_defaults();
    let id = uuid::Uuid::new_v4();
    assert!(!host.is_loaded(id).await);
}
