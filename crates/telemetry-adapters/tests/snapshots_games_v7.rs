//! Snapshot tests for AMS2 and rFactor2 with realistic telemetry data (v7).
//!
//! The existing zeroed-buffer snapshots in `snapshots_extended.rs` verify that
//! the adapters survive all-zero input.  These tests populate fields with
//! representative driving values so the snapshot captures meaningful output.

use racing_wheel_telemetry_adapters::{
    AMS2Adapter, RFactor2Adapter, TelemetryAdapter,
    ams2::AMS2SharedMemory,
    rfactor2::{RF2VehicleTelemetry, RF2WheelTelemetry},
};
use std::mem;
use std::ptr;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Convert a `#[repr(C)]` value to its raw byte representation.
fn struct_to_bytes<T: Copy>(val: &T) -> Vec<u8> {
    let size = mem::size_of::<T>();
    let mut buf = vec![0u8; size];
    // SAFETY: T is Copy + repr(C), buf is exactly size_of::<T>() bytes.
    unsafe {
        ptr::copy_nonoverlapping(val as *const T as *const u8, buf.as_mut_ptr(), size);
    }
    buf
}

/// Write a UTF-8 string into a fixed-size byte buffer (null-terminated).
fn write_string(dst: &mut [u8], s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(dst.len() - 1);
    dst[..len].copy_from_slice(&bytes[..len]);
    dst[len] = 0;
}

// ─── AMS2 ─────────────────────────────────────────────────────────────────────

fn make_ams2_data() -> AMS2SharedMemory {
    let mut data = AMS2SharedMemory::default();

    // Session / state
    data.version = 12;
    data.game_state = 2; // InGamePlaying
    data.session_state = 5; // Race
    data.race_state = 2; // Racing
    data.laps_completed = 3;
    data.laps_in_event = 15;

    // Car dynamics
    data.speed = 45.0; // 45 m/s ≈ 162 km/h
    data.rpm = 7200.0;
    data.max_rpm = 8500.0;
    data.gear = 4;
    data.num_gears = 6;
    data.fuel_level = 32.5;
    data.fuel_capacity = 60.0;

    // Controls & FFB
    data.throttle = 0.75;
    data.brake = 0.0;
    data.clutch = 0.0;
    data.steering = 0.15; // slight right input

    // Electronics
    data.tc_setting = 2;
    data.abs_setting = 1;

    // Tyre slip (non-zero so slip_ratio is computed; speed > 1.0)
    data.tyre_slip = [0.04, 0.05, 0.08, 0.07];

    // Flags: green flag
    data.highest_flag = 1; // Green

    // Car / track names
    write_string(&mut data.car_name, "Formula_Trainer");
    write_string(&mut data.track_location, "Interlagos");

    data
}

#[test]
fn ams2_normalized_snapshot() -> TestResult {
    let adapter = AMS2Adapter::new();
    let raw = struct_to_bytes(&make_ams2_data());
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── rFactor2 ─────────────────────────────────────────────────────────────────

fn make_rf2_vehicle() -> RF2VehicleTelemetry {
    let mut vehicle = RF2VehicleTelemetry::default();

    vehicle.id = 1;
    vehicle.lap_number = 5;
    vehicle.speed = 55.0; // 55 m/s ≈ 198 km/h
    vehicle.gear = 5;
    vehicle.engine_rpm = 9500.0;
    vehicle.engine_max_rpm = 11_000.0;
    vehicle.engine_water_temp = 92.0;
    vehicle.engine_oil_temp = 105.0;
    vehicle.fuel = 28.0;

    // Input controls
    vehicle.unfiltered_throttle = 0.85;
    vehicle.unfiltered_brake = 0.0;
    vehicle.unfiltered_steering = 0.1;
    vehicle.unfiltered_clutch = 0.0;

    // FFB via steering shaft torque (≤ 1.5 → clamped directly)
    vehicle.steering_shaft_torque = 0.75;

    // Wheel slip (non-zero so slip_ratio is computed; speed ≥ 1.0)
    let base_wheel = RF2WheelTelemetry {
        lateral_patch_slip: 0.06,
        ..RF2WheelTelemetry::default()
    };
    vehicle.wheels = [
        RF2WheelTelemetry {
            lateral_patch_slip: 0.04,
            ..base_wheel
        },
        RF2WheelTelemetry {
            lateral_patch_slip: 0.05,
            ..base_wheel
        },
        RF2WheelTelemetry {
            lateral_patch_slip: 0.08,
            ..base_wheel
        },
        RF2WheelTelemetry {
            lateral_patch_slip: 0.07,
            ..base_wheel
        },
    ];

    // Car / track names
    write_string(&mut vehicle.vehicle_name, "Dallara_IR18");
    write_string(&mut vehicle.track_name, "Spa-Francorchamps");

    vehicle
}

#[test]
fn rfactor2_normalized_snapshot() -> TestResult {
    let adapter = RFactor2Adapter::new();
    let raw = struct_to_bytes(&make_rf2_vehicle());
    let normalized = adapter.normalize(&raw)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}
