//! Insta snapshot tests for the `racing-wheel-telemetry-mudrunner` crate.
//!
//! Each test normalizes a representative JSON packet and snapshots the full
//! [`NormalizedTelemetry`] output as YAML.

use racing_wheel_telemetry_mudrunner::{MudRunnerAdapter, MudRunnerVariant, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn snapshot_zero_packet() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn snapshot_mid_drive() -> TestResult {
    let adapter = MudRunnerAdapter::new();
    let json = br#"{"SpeedMs":8.5,"Rpms":2500.0,"MaxRpms":4500.0,"Gear":"2","Throttle":60.0,"Brake":20.0,"Clutch":0.0,"SteeringAngle":-90.0,"FuelPercent":70.0,"LateralGForce":0.3,"LongitudinalGForce":0.5,"FFBValue":0.2,"IsRunning":true,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

#[test]
fn snapshot_snowrunner_reverse() -> TestResult {
    let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
    let json = br#"{"SpeedMs":2.0,"Rpms":1500.0,"MaxRpms":4500.0,"Gear":"R","Throttle":30.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":45.0,"FuelPercent":50.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":true,"IsInPit":false}"#;
    let normalized = adapter.normalize(json)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
