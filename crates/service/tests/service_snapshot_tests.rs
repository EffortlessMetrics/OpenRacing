//! Snapshot tests for service status, health reports, IPC responses,
//! and diagnostic output.

use racing_wheel_service::system_config::{DevelopmentConfig, SafetyConfig, ServiceConfig};
use racing_wheel_service::{
    DeviceState, DiagnosticResult, DiagnosticStatus, FaultSeverity, FeatureFlags, InterlockState,
    SystemConfig,
};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Service status snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_device_state_all_variants() {
    let states = vec![
        DeviceState::Disconnected,
        DeviceState::Connected,
        DeviceState::Ready,
        DeviceState::Faulted {
            reason: "USB communication failure".to_string(),
        },
    ];
    insta::assert_debug_snapshot!("device_state_all_variants", states);
}

#[test]
fn snapshot_interlock_state_safe_torque() {
    insta::assert_debug_snapshot!("interlock_state_safe_torque", InterlockState::SafeTorque);
}

#[test]
fn snapshot_fault_severity_all() {
    let severities = vec![
        FaultSeverity::Warning,
        FaultSeverity::Critical,
        FaultSeverity::Fatal,
    ];
    insta::assert_debug_snapshot!("fault_severity_all", severities);
}

#[test]
fn snapshot_feature_flags_typical() {
    let flags = FeatureFlags {
        disable_realtime: false,
        force_ffb_mode: None,
        enable_dev_features: false,
        enable_debug_logging: false,
        enable_virtual_devices: false,
        disable_safety_interlocks: false,
        enable_plugin_dev_mode: false,
    };
    insta::assert_debug_snapshot!("feature_flags_typical", flags);
}

#[test]
fn snapshot_service_config_default() {
    let config = ServiceConfig::default();
    insta::assert_json_snapshot!("service_config_default", config);
}

// ---------------------------------------------------------------------------
// Health report snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_safety_config_default() {
    let config = SafetyConfig::default();
    insta::assert_json_snapshot!("safety_config_default", config);
}

#[test]
fn snapshot_development_config_default() {
    let config = DevelopmentConfig::default();
    insta::assert_json_snapshot!("development_config_default", config);
}

// ---------------------------------------------------------------------------
// IPC response snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_ipc_config_default() {
    let config = racing_wheel_service::system_config::IpcConfig::default();
    insta::assert_json_snapshot!("ipc_config_default", config);
}

#[test]
fn snapshot_transport_type_native() {
    let transport = racing_wheel_service::system_config::TransportType::Native;
    insta::assert_debug_snapshot!("transport_type_native", transport);
}

#[test]
fn snapshot_transport_type_tcp() {
    let transport = racing_wheel_service::system_config::TransportType::Tcp;
    insta::assert_debug_snapshot!("transport_type_tcp", transport);
}

// ---------------------------------------------------------------------------
// Diagnostic output snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_diagnostic_result_pass() {
    let result = DiagnosticResult {
        name: "USB Connection".to_string(),
        status: DiagnosticStatus::Pass,
        message: "Device communicating at 1kHz".to_string(),
        execution_time_ms: 125,
        metadata: HashMap::new(),
        suggested_actions: vec![],
    };
    insta::assert_json_snapshot!("diagnostic_result_pass", result);
}

#[test]
fn snapshot_diagnostic_result_warn() {
    let mut metadata = HashMap::new();
    metadata.insert("temperature_c".to_string(), "78".to_string());
    metadata.insert("threshold_c".to_string(), "80".to_string());

    let result = DiagnosticResult {
        name: "Temperature Check".to_string(),
        status: DiagnosticStatus::Warn,
        message: "Device temperature approaching limit".to_string(),
        execution_time_ms: 42,
        metadata,
        suggested_actions: vec![
            "Improve ventilation".to_string(),
            "Reduce torque output".to_string(),
        ],
    };
    let mut settings = insta::Settings::clone_current();
    settings.set_sort_maps(true);
    settings.bind(|| {
        insta::assert_json_snapshot!("diagnostic_result_warn", result);
    });
}

#[test]
fn snapshot_diagnostic_result_fail() {
    let result = DiagnosticResult {
        name: "Encoder Health".to_string(),
        status: DiagnosticStatus::Fail,
        message: "Encoder NaN values detected".to_string(),
        execution_time_ms: 5,
        metadata: HashMap::from([("fault_code".to_string(), "0x02".to_string())]),
        suggested_actions: vec![
            "Check encoder cable".to_string(),
            "Power-cycle device".to_string(),
        ],
    };
    let mut settings = insta::Settings::clone_current();
    settings.set_sort_maps(true);
    settings.bind(|| {
        insta::assert_json_snapshot!("diagnostic_result_fail", result);
    });
}

#[test]
fn snapshot_diagnostic_status_all_debug() {
    let statuses = vec![
        DiagnosticStatus::Pass,
        DiagnosticStatus::Warn,
        DiagnosticStatus::Fail,
    ];
    insta::assert_debug_snapshot!("diagnostic_status_all", statuses);
}

#[test]
fn snapshot_system_config_schema_version() {
    let config = SystemConfig::default();
    insta::assert_snapshot!("system_config_schema_version", config.schema_version);
}

#[test]
fn snapshot_system_config_engine_default() {
    let config = SystemConfig::default();
    insta::assert_json_snapshot!("system_config_engine", config.engine);
}
