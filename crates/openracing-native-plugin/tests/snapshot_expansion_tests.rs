//! Snapshot tests for NativePluginLoadError — ensure error messages are stable.
//! NativePluginError has non-Clone variants (libloading::Error, io::Error) so
//! we focus on NativePluginLoadError which is Clone and covers the key safety messages.

use openracing_native_plugin::error::NativePluginLoadError;

#[test]
fn snapshot_plugin_load_error_abi_mismatch() {
    let err = NativePluginLoadError::AbiMismatch {
        expected: 3,
        actual: 1,
    };
    insta::assert_snapshot!("plugin_load_abi_mismatch", format!("{}", err));
}

#[test]
fn snapshot_plugin_load_error_invalid_signature() {
    let err = NativePluginLoadError::InvalidSignature {
        reason: "signature expired".to_string(),
    };
    insta::assert_snapshot!("plugin_load_invalid_signature", format!("{}", err));
}

#[test]
fn snapshot_plugin_load_error_unsigned() {
    let err = NativePluginLoadError::UnsignedPlugin {
        path: "/opt/plugins/my_plugin.so".to_string(),
    };
    insta::assert_snapshot!("plugin_load_unsigned", format!("{}", err));
}

#[test]
fn snapshot_plugin_load_error_untrusted_signer() {
    let err = NativePluginLoadError::UntrustedSigner {
        fingerprint: "SHA256:abc123def456".to_string(),
    };
    insta::assert_snapshot!("plugin_load_untrusted_signer", format!("{}", err));
}

#[test]
fn snapshot_plugin_load_error_library_load_failed() {
    let err = NativePluginLoadError::LibraryLoadFailed {
        reason: "symbol not found: plugin_init".to_string(),
    };
    insta::assert_snapshot!("plugin_load_library_failed", format!("{}", err));
}

#[test]
fn snapshot_plugin_load_error_init_failed() {
    let err = NativePluginLoadError::InitializationFailed {
        reason: "shared memory allocation failed".to_string(),
    };
    insta::assert_snapshot!("plugin_load_init_failed", format!("{}", err));
}

// --- NativePluginError constructible variants ---

use openracing_native_plugin::error::NativePluginError;

#[test]
fn snapshot_native_plugin_error_loading_failed() {
    let err = NativePluginError::LoadingFailed("file not found".to_string());
    insta::assert_snapshot!("native_plugin_loading_failed", format!("{}", err));
}

#[test]
fn snapshot_native_plugin_error_abi_mismatch() {
    let err = NativePluginError::AbiMismatch {
        expected: 3,
        actual: 2,
    };
    insta::assert_snapshot!("native_plugin_abi_mismatch", format!("{}", err));
}

#[test]
fn snapshot_native_plugin_error_unsigned() {
    let err = NativePluginError::UnsignedPlugin {
        path: std::path::PathBuf::from("/opt/plugins/unsigned.so"),
    };
    insta::assert_snapshot!("native_plugin_unsigned", format!("{}", err));
}

#[test]
fn snapshot_native_plugin_error_execution_timeout() {
    let err = NativePluginError::ExecutionTimeout { duration_us: 500 };
    insta::assert_snapshot!("native_plugin_execution_timeout", format!("{}", err));
}

#[test]
fn snapshot_native_plugin_error_budget_violation() {
    let err = NativePluginError::BudgetViolation {
        used_us: 150,
        budget_us: 100,
    };
    insta::assert_snapshot!("native_plugin_budget_violation", format!("{}", err));
}

#[test]
fn snapshot_native_plugin_error_crashed() {
    let err = NativePluginError::Crashed {
        reason: "SIGSEGV in plugin code".to_string(),
    };
    insta::assert_snapshot!("native_plugin_crashed", format!("{}", err));
}

#[test]
fn snapshot_native_plugin_error_shared_memory() {
    let err = NativePluginError::SharedMemoryError("region already mapped".to_string());
    insta::assert_snapshot!("native_plugin_shared_memory", format!("{}", err));
}

#[test]
fn snapshot_native_plugin_error_config() {
    let err = NativePluginError::ConfigError("invalid timeout value".to_string());
    insta::assert_snapshot!("native_plugin_config_error", format!("{}", err));
}
