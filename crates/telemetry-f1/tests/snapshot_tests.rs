//! Insta snapshot tests for the F1 2024 telemetry adapter.
//!
//! These tests lock down the normalized output format so that any change to
//! the adapter's output is caught as a snapshot diff.

use racing_wheel_telemetry_adapters::f1_25::{CarTelemetryData, SessionData};
use racing_wheel_telemetry_adapters::f1_native::{
    F1NativeAdapter, F1NativeCarStatusData, PACKET_FORMAT_2024, build_car_telemetry_packet_native,
    normalize,
};
use racing_wheel_telemetry_f1::TelemetryAdapter;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Snapshot the full normalized output from a realistic F1 2024 mid-race frame.
#[test]
fn snapshot_f1_2024_normalize_mid_race() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 285,
        throttle: 0.92,
        steer: -0.08,
        brake: 0.0,
        gear: 7,
        engine_rpm: 11800,
        drs: 1,
        brakes_temperature: [450, 460, 420, 430],
        tyres_surface_temperature: [95, 96, 88, 89],
        tyres_inner_temperature: [105, 106, 98, 99],
        engine_temperature: 108,
        tyres_pressure: [23.5, 23.8, 22.0, 22.3],
    };
    let status = F1NativeCarStatusData {
        fuel_in_tank: 32.5,
        fuel_remaining_laps: 12.3,
        actual_tyre_compound: 16,
        tyre_age_laps: 8,
        ers_store_energy: 2_800_000.0,
        ers_deploy_mode: 3,
        ers_harvested_mguk: 600_000.0,
        ers_harvested_mguh: 400_000.0,
        ers_deployed: 1_200_000.0,
        engine_power_ice: 560_000.0,
        engine_power_mguk: 120_000.0,
        max_rpm: 13500,
        drs_allowed: 1,
        pit_limiter_status: 0,
        traction_control: 0,
        anti_lock_brakes: 0,
    };
    let session = SessionData {
        track_id: 14,
        session_type: 10,
        track_temperature: 38,
        air_temperature: 28,
    };

    let norm = normalize(&telem, &status, &session);
    insta::assert_yaml_snapshot!("f1_2024_mid_race_frame", norm);
    Ok(())
}

/// Snapshot adapter.normalize() with a car telemetry packet.
#[test]
fn snapshot_f1_adapter_normalize_telemetry_packet() -> TestResult {
    let adapter = F1NativeAdapter::new();
    let raw = build_car_telemetry_packet_native(
        PACKET_FORMAT_2024,
        0,
        220,
        6,
        10500,
        0.85,
        0.0,
        -0.12,
        0,
        [23.0, 23.2, 21.8, 22.0],
    );
    let norm = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!("f1_adapter_telemetry_packet", norm);
    Ok(())
}

/// Snapshot a pit-limiter active scenario.
#[test]
fn snapshot_f1_2024_pit_entry() -> TestResult {
    let telem = CarTelemetryData {
        speed_kmh: 80,
        throttle: 0.3,
        steer: 0.02,
        brake: 0.0,
        gear: 3,
        engine_rpm: 6500,
        drs: 0,
        brakes_temperature: [280, 290, 260, 270],
        tyres_surface_temperature: [82, 83, 78, 79],
        tyres_inner_temperature: [92, 93, 88, 89],
        engine_temperature: 102,
        tyres_pressure: [22.5, 22.8, 21.5, 21.8],
    };
    let status = F1NativeCarStatusData {
        fuel_in_tank: 15.0,
        fuel_remaining_laps: 4.2,
        actual_tyre_compound: 18,
        tyre_age_laps: 22,
        ers_store_energy: 500_000.0,
        ers_deploy_mode: 0,
        pit_limiter_status: 1,
        drs_allowed: 0,
        max_rpm: 13500,
        ..F1NativeCarStatusData::default()
    };
    let session = SessionData {
        track_id: 11,
        session_type: 10,
        track_temperature: 30,
        air_temperature: 24,
    };

    let norm = normalize(&telem, &status, &session);
    insta::assert_yaml_snapshot!("f1_2024_pit_entry_frame", norm);
    Ok(())
}
