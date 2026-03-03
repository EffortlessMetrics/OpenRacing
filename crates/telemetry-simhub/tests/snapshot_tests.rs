//! Insta snapshot tests for the `racing-wheel-telemetry-simhub` crate.
//!
//! Each test normalizes a representative JSON packet and snapshots the full
//! [`NormalizedTelemetry`] output as YAML.

use racing_wheel_telemetry_simhub::{SimHubAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn snapshot_zero_packet() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn snapshot_full_telemetry() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":22.5,"Rpms":4500.0,"MaxRpms":8000.0,"Gear":"3","Throttle":75.0,"Brake":10.0,"Clutch":0.0,"SteeringAngle":-90.0,"FuelPercent":82.3,"LateralGForce":1.2,"LongitudinalGForce":-0.5,"FFBValue":0.35,"IsRunning":true,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn snapshot_full_lock_reverse() -> TestResult {
    let adapter = SimHubAdapter::new();
    let json = br#"{"SpeedMs":5.0,"Rpms":2000.0,"MaxRpms":8000.0,"Gear":"R","Throttle":100.0,"Brake":50.0,"Clutch":25.0,"SteeringAngle":-450.0,"FuelPercent":100.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":true,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
