//! Snapshot tests for WatchdogError and health types — ensure error messages and
//! health status display output are stable.

use openracing_watchdog::error::WatchdogError;
use openracing_watchdog::health::{HealthStatus, SystemComponent};
use std::time::Duration;

// --- WatchdogError Display (all variants) ---

#[test]
fn snapshot_watchdog_error_plugin_not_found() {
    let err = WatchdogError::plugin_not_found("telemetry-plugin");
    insta::assert_snapshot!("watchdog_error_plugin_not_found", format!("{}", err));
}

#[test]
fn snapshot_watchdog_error_component_not_found() {
    let err = WatchdogError::component_not_found(SystemComponent::RtThread);
    insta::assert_snapshot!("watchdog_error_component_not_found", format!("{}", err));
}

#[test]
fn snapshot_watchdog_error_already_quarantined() {
    let err = WatchdogError::already_quarantined("bad-plugin");
    insta::assert_snapshot!("watchdog_error_already_quarantined", format!("{}", err));
}

#[test]
fn snapshot_watchdog_error_not_quarantined() {
    let err = WatchdogError::not_quarantined("good-plugin");
    insta::assert_snapshot!("watchdog_error_not_quarantined", format!("{}", err));
}

#[test]
fn snapshot_watchdog_error_invalid_configuration() {
    let err = WatchdogError::invalid_configuration("timeout must be > 0");
    insta::assert_snapshot!("watchdog_error_invalid_config", format!("{}", err));
}

#[test]
fn snapshot_watchdog_error_health_check_failed() {
    let err =
        WatchdogError::health_check_failed(SystemComponent::HidCommunication, "USB disconnected");
    insta::assert_snapshot!("watchdog_error_health_check_failed", format!("{}", err));
}

#[test]
fn snapshot_watchdog_error_quarantine_failed() {
    let err = WatchdogError::quarantine_failed("isolation barrier unavailable");
    insta::assert_snapshot!("watchdog_error_quarantine_failed", format!("{}", err));
}

#[test]
fn snapshot_watchdog_error_timeout_exceeded() {
    let err = WatchdogError::timeout_exceeded("plugin_init", Duration::from_secs(30));
    insta::assert_snapshot!("watchdog_error_timeout_exceeded", format!("{}", err));
}

#[test]
fn snapshot_watchdog_error_callback_registration() {
    let err = WatchdogError::CallbackRegistrationFailed("handler already registered".to_string());
    insta::assert_snapshot!("watchdog_error_callback_registration", format!("{}", err));
}

#[test]
fn snapshot_watchdog_error_stats_failed() {
    let err = WatchdogError::StatsFailed("counter overflow".to_string());
    insta::assert_snapshot!("watchdog_error_stats_failed", format!("{}", err));
}

// --- HealthStatus Display (all 4 variants) ---

#[test]
fn snapshot_health_status_display() {
    insta::assert_snapshot!(
        "health_status_healthy",
        format!("{}", HealthStatus::Healthy)
    );
    insta::assert_snapshot!(
        "health_status_degraded",
        format!("{}", HealthStatus::Degraded)
    );
    insta::assert_snapshot!(
        "health_status_faulted",
        format!("{}", HealthStatus::Faulted)
    );
    insta::assert_snapshot!(
        "health_status_unknown",
        format!("{}", HealthStatus::Unknown)
    );
}

// --- SystemComponent Display (all 6 variants) ---

#[test]
fn snapshot_system_component_display() {
    insta::assert_snapshot!(
        "system_component_rt_thread",
        format!("{}", SystemComponent::RtThread)
    );
    insta::assert_snapshot!(
        "system_component_hid",
        format!("{}", SystemComponent::HidCommunication)
    );
    insta::assert_snapshot!(
        "system_component_telemetry",
        format!("{}", SystemComponent::TelemetryAdapter)
    );
    insta::assert_snapshot!(
        "system_component_plugin_host",
        format!("{}", SystemComponent::PluginHost)
    );
    insta::assert_snapshot!(
        "system_component_safety",
        format!("{}", SystemComponent::SafetySystem)
    );
    insta::assert_snapshot!(
        "system_component_device_mgr",
        format!("{}", SystemComponent::DeviceManager)
    );
}
